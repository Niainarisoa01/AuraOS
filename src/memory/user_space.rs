//! ============================================================================
//! User Space Memory Manager — Per-Process Page Tables & Address Isolation
//! ============================================================================
//!
//! Provides isolated virtual address spaces for Ring 3 user processes.
//! Each user process owns an `AddressSpace` consisting of a dedicated PML4
//! hierarchy where:
//!   1. Kernel code/data/stacks are preserved in the upper hierarchy or supervisor entries.
//!   2. User code, data, and stacks are explicitly marked with `page_flags::USER_ACCESSIBLE` (bit 2).
//!   3. The CPU hardware enforces that Ring 3 code cannot access any memory lacking `USER_ACCESSIBLE`.
//!
//! CRITICAL: The bootloader does NOT identity-map the kernel. Virtual addresses
//! returned by the Rust allocator are NOT equal to their physical addresses.
//! Page tables and user pages are therefore allocated from the Physical Memory
//! Manager (`memory::pmm`), whose frames live in the identity-mapped window
//! [2 MiB .. 512 MiB) — there, physical address == virtual address, so the
//! kernel can access them directly.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::memory::paging::{
    read_cr3, write_cr3, PhysAddr, VirtAddr, PageTable, page_flags, PAGE_SIZE,
};
#[allow(unused_imports)]
use crate::memory::vmm::{
    VmArea, VmmError, PROT_NONE, PROT_READ, PROT_WRITE, PROT_EXEC, MAP_PRIVATE, MAP_SHARED,
    MAP_ANONYMOUS, MAP_FIXED, MAP_POPULATE, MMAP_BASE_START, MMAP_BASE_END, align_up_page,
    align_down_page,
};

/// Global snapshot of the initial kernel CR3 register.
pub static KERNEL_CR3: AtomicU64 = AtomicU64::new(0);

/// Initializes the user space memory management subsystem by recording the kernel CR3.
pub fn init() {
    let cr3 = read_cr3();
    KERNEL_CR3.store(cr3.as_u64(), Ordering::SeqCst);
    crate::println!("[OK] UserSpace : Memory isolation manager initialized (Kernel CR3: {:#x}).", cr3.as_u64());
    crate::serial_println!("[OK] UserSpace: Memory manager ready (CR3: {:#x})", cr3.as_u64());
}

/// A page table allocation with both its virtual address (for kernel access)
/// and its physical address (for CPU page-table walks / CR3).
struct PageTableAlloc {
    virt: *mut PageTable,
    phys: PhysAddr,
}

/// Allocates a zeroed, 4096-byte page table aligned to 4 KiB.
/// The frame comes from the Physical Memory Manager and lives in the
/// identity-mapped window [2 MiB .. 512 MiB): its physical address can be
/// used directly as a virtual pointer by the kernel.
/// Returns both the pointer (for kernel read/write) and the
/// physical address (for use in page table entries and CR3).
unsafe fn alloc_page_table() -> Option<PageTableAlloc> {
    let phys = crate::memory::pmm::allocate_frame()?;
    let ptr = phys.as_u64() as *mut PageTable;
    unsafe {
        ptr.write_bytes(0u8, 1); // zero the 4 KiB frame
    }
    Some(PageTableAlloc {
        virt: ptr,
        phys,
    })
}

/// Dumps the 4-level page table walk for a virtual address under a specific CR3 root.
/// NOTE: This walks using physical addresses as virtual pointers, which only works
/// for page table pages that are identity-mapped (bootloader's own tables at low memory).
#[allow(dead_code)]
pub fn debug_dump_walk(cr3: u64, virt: u64) {
    let p4_idx = ((virt >> 39) & 0x1FF) as usize;
    let p3_idx = ((virt >> 30) & 0x1FF) as usize;
    let p2_idx = ((virt >> 21) & 0x1FF) as usize;
    let p1_idx = ((virt >> 12) & 0x1FF) as usize;
    let pml4 = cr3 as *const PageTable;
    let p4e = unsafe { (*pml4).entries[p4_idx] };
    crate::serial_println!("[WALK] CR3={:#x}, Virt={:#x}: PML4[{}]={:#x}", cr3, virt, p4_idx, p4e.0);
    if !p4e.is_present() { return; }
    let pdpt = p4e.addr().as_u64() as *const PageTable;
    let p3e = unsafe { (*pdpt).entries[p3_idx] };
    crate::serial_println!("[WALK] PDPT[{}]={:#x}", p3_idx, p3e.0);
    if !p3e.is_present() || (p3e.flags() & page_flags::HUGE_PAGE) != 0 { return; }
    let pd = p3e.addr().as_u64() as *const PageTable;
    let p2e = unsafe { (*pd).entries[p2_idx] };
    crate::serial_println!("[WALK] PD[{}]={:#x}", p2_idx, p2e.0);
    if !p2e.is_present() || (p2e.flags() & page_flags::HUGE_PAGE) != 0 { return; }
    let pt = p2e.addr().as_u64() as *const PageTable;
    let p1e = unsafe { (*pt).entries[p1_idx] };
    crate::serial_println!("[WALK] PT[{}]={:#x}", p1_idx, p1e.0);
}

/// Default virtual memory base for dynamic user heap allocation (1.25 GiB).
pub const DEFAULT_USER_HEAP_START: u64 = 0x0000_0000_5000_0000;

/// Represents an isolated virtual memory address space for a user process.
#[allow(dead_code)]
pub struct AddressSpace {
    /// Virtual pointer to the PML4 table (for kernel read/write access)
    pml4_virt: *mut PageTable,
    /// Physical address of the PML4 table (for CR3 and CPU page walks)
    pml4_phys: PhysAddr,
    /// Physical addresses of every frame allocated from the PMM
    /// (page tables + user pages), freed on `Drop`.
    allocated_frames: Vec<u64>,
    /// Virtual memory areas registered for this process
    vmas: Vec<VmArea>,
    /// Next address hint for anonymous mmap allocations
    mmap_bump_ptr: u64,
    /// Starting virtual address for the process's heap
    pub heap_start: u64,
    /// Current break point for the process's heap
    pub heap_end: u64,
}

unsafe impl Send for AddressSpace {}
unsafe impl Sync for AddressSpace {}

#[allow(dead_code)]
impl AddressSpace {
    /// Clones the kernel mapping into a newly allocated PML4 table.
    /// Preserves all upper supervisor entries and establishes an isolated
    /// PDPT for PML4[0] to prevent user mappings from mutating the kernel's tables.
    unsafe fn clone_kernel_mapping(
        pml4_virt: *mut PageTable,
        allocated_frames: &mut Vec<u64>,
    ) -> Option<()> {
        let kcr3 = KERNEL_CR3.load(Ordering::Relaxed);
        let kernel_cr3 = if kcr3 != 0 { PhysAddr(kcr3) } else { read_cr3() };
        let kernel_pml4 = kernel_cr3.as_u64() as *const PageTable;

        unsafe {
            // Step 1: Copy ALL kernel PML4 entries to preserve kernel mappings
            // (interrupts, GDT, IDT, kernel stacks, bootloader-mapped regions)
            for i in 0..512 {
                (*pml4_virt).entries[i] = (*kernel_pml4).entries[i];
            }

            // Step 2: For PML4[0], clone the PDPT so user mappings don't mutate the kernel's table
            let kernel_pml4_0 = (*kernel_pml4).entries[0];
            if kernel_pml4_0.is_present() {
                let kernel_pdpt_phys = kernel_pml4_0.addr().as_u64();
                let kernel_pdpt = kernel_pdpt_phys as *const PageTable;

                let user_pdpt = alloc_page_table()?;
                allocated_frames.push(user_pdpt.phys.as_u64());

                // Copy all kernel PDPT entries
                for i in 0..512 {
                    (*user_pdpt.virt).entries[i] = (*kernel_pdpt).entries[i];
                }

                // Point PML4[0] to the new user PDPT using its PHYSICAL address
                (*pml4_virt).entries[0].set(
                    user_pdpt.phys,
                    page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER_ACCESSIBLE,
                );
            }
        }
        Some(())
    }

    /// Creates a new isolated address space with kernel mappings preserved.
    pub fn new() -> Option<Self> {
        let pml4 = unsafe { alloc_page_table()? };
        let mut allocated_frames = Vec::new();
        allocated_frames.push(pml4.phys.as_u64());

        let pml4_virt = pml4.virt;
        let pml4_phys = pml4.phys;

        unsafe {
            Self::clone_kernel_mapping(pml4_virt, &mut allocated_frames)?;
        }

        Some(AddressSpace {
            pml4_virt,
            pml4_phys,
            allocated_frames,
            vmas: Vec::new(),
            mmap_bump_ptr: MMAP_BASE_START,
            heap_start: DEFAULT_USER_HEAP_START,
            heap_end: DEFAULT_USER_HEAP_START,
        })
    }

    /// Returns the virtual address corresponding to a physical address of a page table.
    /// All frames allocated by this AddressSpace (and the bootloader's own low
    /// tables) live in identity-mapped windows, so virtual == physical here.
    fn phys_to_virt(&self, phys: u64) -> *mut PageTable {
        phys as *mut PageTable
    }

    /// Returns the virtual pointer to the PML4 root table.
    pub fn pml4_virt(&self) -> *const PageTable {
        self.pml4_virt
    }

    /// Returns the physical address of this address space's PML4 root table.
    pub fn pml4_phys(&self) -> PhysAddr {
        self.pml4_phys
    }

    /// Maps a 4 KiB virtual page to a physical frame with user privileges.
    ///
    /// `virt`: the user virtual address to map
    /// `phys`: the PHYSICAL address of the target frame
    /// `writable`: whether the page should be writable
    pub fn map_user_page(&mut self, virt: VirtAddr, phys: PhysAddr, writable: bool) -> bool {
        let p4_idx = virt.p4_index();
        let p3_idx = virt.p3_index();
        let p2_idx = virt.p2_index();
        let p1_idx = virt.p1_index();

        unsafe {
            let pml4 = self.pml4_virt;

            // 1. Level 4 (PML4) -> Level 3 (PDPT)
            let p3_table: *mut PageTable = if (*pml4).entries[p4_idx].is_present() {
                let cur_flags = (*pml4).entries[p4_idx].flags();
                let addr = (*pml4).entries[p4_idx].addr();
                (*pml4).entries[p4_idx].set(addr, cur_flags | page_flags::USER_ACCESSIBLE | page_flags::WRITABLE);
                self.phys_to_virt(addr.as_u64())
            } else {
                let new_table = match alloc_page_table() {
                    Some(t) => t,
                    None => return false,
                };
                self.allocated_frames.push(new_table.phys.as_u64());
                (*pml4).entries[p4_idx].set(
                    new_table.phys,  // Store PHYSICAL address in PTE
                    page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER_ACCESSIBLE,
                );
                new_table.virt  // Return VIRTUAL address for kernel access
            };

            // 2. Level 3 (PDPT) -> Level 2 (PD)
            let p2_table: *mut PageTable = if (*p3_table).entries[p3_idx].is_present() {
                let cur_flags = (*p3_table).entries[p3_idx].flags();
                let addr = (*p3_table).entries[p3_idx].addr();
                (*p3_table).entries[p3_idx].set(addr, cur_flags | page_flags::USER_ACCESSIBLE | page_flags::WRITABLE);
                self.phys_to_virt(addr.as_u64())
            } else {
                let new_table = match alloc_page_table() {
                    Some(t) => t,
                    None => return false,
                };
                self.allocated_frames.push(new_table.phys.as_u64());
                (*p3_table).entries[p3_idx].set(
                    new_table.phys,
                    page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER_ACCESSIBLE,
                );
                new_table.virt
            };

            // 3. Level 2 (PD) -> Level 1 (PT)
            let p1_table: *mut PageTable = if (*p2_table).entries[p2_idx].is_present() {
                let cur_flags = (*p2_table).entries[p2_idx].flags();
                let addr = (*p2_table).entries[p2_idx].addr();
                (*p2_table).entries[p2_idx].set(addr, cur_flags | page_flags::USER_ACCESSIBLE | page_flags::WRITABLE);
                self.phys_to_virt(addr.as_u64())
            } else {
                let new_table = match alloc_page_table() {
                    Some(t) => t,
                    None => return false,
                };
                self.allocated_frames.push(new_table.phys.as_u64());
                (*p2_table).entries[p2_idx].set(
                    new_table.phys,
                    page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER_ACCESSIBLE,
                );
                new_table.virt
            };

            // 4. Level 1 (PT) -> Physical Page Frame
            let mut flags = page_flags::PRESENT | page_flags::USER_ACCESSIBLE;
            if writable {
                flags |= page_flags::WRITABLE;
            }
            (*p1_table).entries[p1_idx].set(phys, flags);
        }

        true
    }

    /// Allocates a new physical frame from the PMM and maps it into user space.
    /// The frame lives in the identity-mapped window, so its physical address
    /// is directly usable as a kernel pointer (for ELF code copies etc.).
    pub fn allocate_and_map_page(&mut self, virt: VirtAddr, writable: bool) -> Option<*mut u8> {
        let phys = crate::memory::pmm::allocate_frame()?;

        let frame_ptr = phys.as_u64() as *mut u8;
        unsafe {
            frame_ptr.write_bytes(0u8, 1); // zero the frame before handing it to user space
        }
        self.allocated_frames.push(phys.as_u64());

        if self.map_user_page(virt, phys, writable) {
            Some(frame_ptr)
        } else {
            None
        }
    }

    /// Allocates and copies executable machine code to the specified user virtual address.
    pub fn allocate_user_code(&mut self, base_virt: VirtAddr, code: &[u8]) -> bool {
        let mut offset = 0;
        while offset < code.len() {
            let page_virt = VirtAddr(base_virt.as_u64() + offset as u64);
            let frame = match self.allocate_and_map_page(page_virt, true) {
                Some(f) => f,
                None => return false,
            };

            let bytes_to_copy = core::cmp::min(PAGE_SIZE, code.len() - offset);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    code.as_ptr().add(offset),
                    frame,
                    bytes_to_copy,
                );
            }
            offset += PAGE_SIZE;
        }
        true
    }

    /// Allocates a user-space stack of `num_pages` pages ending at `top_virt`,
    /// registers a [stack] VMA, and installs a guard page directly beneath it.
    pub fn allocate_user_stack(&mut self, top_virt: VirtAddr, num_pages: usize) -> bool {
        let stack_size = num_pages * PAGE_SIZE;
        let stack_base = top_virt.as_u64() - stack_size as u64;

        // 1. Install 4 KiB unmapped guard page directly below the stack
        let guard_base = stack_base - PAGE_SIZE as u64;
        let _ = self.add_guard_page(guard_base, PAGE_SIZE);

        // 2. Allocate and map user stack physical frames
        for i in 1..=num_pages {
            let page_virt = VirtAddr(top_virt.as_u64() - (i * PAGE_SIZE) as u64);
            if self.allocate_and_map_page(page_virt, true).is_none() {
                return false;
            }
        }

        // 3. Register [stack] VMA
        let mut vma = VmArea::new(
            stack_base,
            stack_size,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            false,
            "[stack]",
        );
        for i in 1..=num_pages {
            let page_virt = top_virt.as_u64() - (i * PAGE_SIZE) as u64;
            // Record populated page
            vma.populated_pages.insert(page_virt, 0); // physical addr tracked in allocated_frames
        }
        let _ = self.add_vma(vma);

        true
    }

    /// Returns a slice of all active Virtual Memory Areas.
    pub fn vmas(&self) -> &[VmArea] {
        &self.vmas
    }

    /// Returns a mutable reference to the list of Virtual Memory Areas.
    pub fn vmas_mut(&mut self) -> &mut Vec<VmArea> {
        &mut self.vmas
    }

    /// Registers a new Virtual Memory Area, ensuring no overlaps exist.
    pub fn add_vma(&mut self, vma: VmArea) -> Result<(), VmmError> {
        if vma.size == 0 || (vma.start & (PAGE_SIZE as u64 - 1)) != 0 {
            return Err(VmmError::InvalidArguments);
        }
        for existing in &self.vmas {
            if existing.overlaps(vma.start, vma.size) {
                return Err(VmmError::Overlap);
            }
        }
        self.vmas.push(vma);
        Ok(())
    }

    /// Locates the VMA containing the given virtual address.
    pub fn find_vma(&self, addr: u64) -> Option<&VmArea> {
        self.vmas.iter().find(|vma| vma.contains(addr))
    }

    /// Locates a mutable reference to the VMA containing the given virtual address.
    pub fn find_vma_mut(&mut self, addr: u64) -> Option<&mut VmArea> {
        self.vmas.iter_mut().find(|vma| vma.contains(addr))
    }

    /// Returns true if the address corresponds to an unmapped guard page.
    pub fn is_guard_page(&self, addr: u64) -> bool {
        if let Some(vma) = self.find_vma(addr) {
            vma.is_guard_page || vma.prot == PROT_NONE
        } else {
            false
        }
    }

    /// Registers an unmapped guard page (PROT_NONE) to detect stack overflows.
    pub fn add_guard_page(&mut self, guard_addr: u64, size: usize) -> Result<(), VmmError> {
        let vma = VmArea::new(
            guard_addr,
            size,
            PROT_NONE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            true,
            "[guard]",
        );
        self.add_vma(vma)
    }

    /// Unmaps a single 4 KiB virtual page and returns its physical address if it was present.
    pub fn unmap_user_page(&mut self, virt: VirtAddr) -> Option<PhysAddr> {
        let p4_idx = virt.p4_index();
        let p3_idx = virt.p3_index();
        let p2_idx = virt.p2_index();
        let p1_idx = virt.p1_index();

        unsafe {
            let pml4 = self.pml4_virt;
            if !(*pml4).entries[p4_idx].is_present() {
                return None;
            }

            let p3_phys = (*pml4).entries[p4_idx].addr().as_u64();
            let p3_table = self.phys_to_virt(p3_phys);
            if !(*p3_table).entries[p3_idx].is_present() {
                return None;
            }

            let p2_phys = (*p3_table).entries[p3_idx].addr().as_u64();
            let p2_table = self.phys_to_virt(p2_phys);
            if !(*p2_table).entries[p2_idx].is_present() {
                return None;
            }

            let p1_phys = (*p2_table).entries[p2_idx].addr().as_u64();
            let p1_table = self.phys_to_virt(p1_phys);
            let entry = &mut (*p1_table).entries[p1_idx];
            if !entry.is_present() {
                return None;
            }

            let phys = entry.addr();
            entry.set_unused();

            // Invalidate TLB entry for this virtual address
            crate::memory::paging::flush_tlb_page(virt);

            // Remove from allocated_frames tracking so Drop won't double-free it
            if let Some(pos) = self.allocated_frames.iter().position(|&x| x == phys.as_u64()) {
                self.allocated_frames.remove(pos);
            }

            Some(phys)
        }
    }

    /// Allocates a virtual memory region (POSIX mmap).
    /// If MAP_ANONYMOUS without MAP_POPULATE, physical frames are NOT allocated
    /// immediately (Demand Paging via #PF page fault).
    pub fn mmap(
        &mut self,
        addr_hint: Option<u64>,
        length: usize,
        prot: u32,
        flags: u32,
    ) -> Result<u64, VmmError> {
        if length == 0 {
            return Err(VmmError::InvalidArguments);
        }

        let aligned_len = align_up_page(length as u64) as usize;
        let vaddr = if (flags & MAP_FIXED) != 0 {
            let req = addr_hint.ok_or(VmmError::InvalidAddress)?;
            if (req & (PAGE_SIZE as u64 - 1)) != 0 {
                return Err(VmmError::InvalidAddress);
            }
            // Check overlaps
            for existing in &self.vmas {
                if existing.overlaps(req, aligned_len) {
                    return Err(VmmError::Overlap);
                }
            }
            req
        } else {
            // Find a free virtual range starting at hint or bump pointer
            let mut candidate = addr_hint
                .map(align_up_page)
                .filter(|&a| a >= MMAP_BASE_START && a < MMAP_BASE_END)
                .unwrap_or_else(|| align_up_page(self.mmap_bump_ptr));

            if candidate < MMAP_BASE_START {
                candidate = MMAP_BASE_START;
            }

            let mut found = false;
            while candidate + aligned_len as u64 <= MMAP_BASE_END {
                let mut conflict = false;
                for existing in &self.vmas {
                    if existing.overlaps(candidate, aligned_len) {
                        candidate = align_up_page(existing.end());
                        conflict = true;
                        break;
                    }
                }
                if !conflict {
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(VmmError::OutOfMemory);
            }

            self.mmap_bump_ptr = align_up_page(candidate + aligned_len as u64);
            candidate
        };

        let mut vma = VmArea::new(vaddr, aligned_len, prot, flags, false, "[mmap]");

        // If MAP_POPULATE was explicitly requested, eagerly allocate and map all frames
        if (flags & MAP_POPULATE) != 0 {
            let mut offset = 0;
            while offset < aligned_len {
                let page_virt = vaddr + offset as u64;
                let phys = match crate::memory::pmm::allocate_frame() {
                    Some(p) => p,
                    None => {
                        let _ = self.munmap(vaddr, offset);
                        return Err(VmmError::OutOfMemory);
                    }
                };

                unsafe {
                    (phys.as_u64() as *mut u8).write_bytes(0, PAGE_SIZE);
                }
                self.allocated_frames.push(phys.as_u64());

                let writable = (prot & PROT_WRITE) != 0;
                if !self.map_user_page(VirtAddr(page_virt), phys, writable) {
                    let _ = crate::memory::pmm::free_frame(phys);
                    let _ = self.munmap(vaddr, offset);
                    return Err(VmmError::OutOfMemory);
                }

                vma.populated_pages.insert(page_virt, phys.as_u64());
                offset += PAGE_SIZE;
            }
        }

        self.vmas.push(vma);
        Ok(vaddr)
    }

    /// Releases a virtual memory region (POSIX munmap).
    /// Unmaps present pages, returns physical frames to PMM, and updates or removes VMAs.
    pub fn munmap(&mut self, addr: u64, length: usize) -> Result<(), VmmError> {
        if length == 0 || (addr & (PAGE_SIZE as u64 - 1)) != 0 {
            return Err(VmmError::InvalidArguments);
        }

        let aligned_len = align_up_page(length as u64) as usize;
        let unmap_start = addr;
        let unmap_end = addr + aligned_len as u64;

        // 1. Unmap and free physical frames in the range
        let mut curr = unmap_start;
        while curr < unmap_end {
            if let Some(phys) = self.unmap_user_page(VirtAddr(curr)) {
                let _ = crate::memory::pmm::free_frame(phys);
            }
            curr += PAGE_SIZE as u64;
        }

        // 2. Update or remove affected VMAs
        let mut i = 0;
        while i < self.vmas.len() {
            let vma = &mut self.vmas[i];
            if !vma.overlaps(unmap_start, aligned_len) {
                i += 1;
                continue;
            }

            // Remove populated pages in the unmapped range
            let mut keys_to_remove = Vec::new();
            for &page_addr in vma.populated_pages.keys() {
                if page_addr >= unmap_start && page_addr < unmap_end {
                    keys_to_remove.push(page_addr);
                }
            }
            for k in keys_to_remove {
                vma.populated_pages.remove(&k);
            }

            let vma_start = vma.start;
            let vma_end = vma.end();

            if unmap_start <= vma_start && unmap_end >= vma_end {
                // Entire VMA is covered -> remove it
                self.vmas.remove(i);
            } else if unmap_start <= vma_start && unmap_end < vma_end {
                // Trim the beginning of VMA
                vma.start = unmap_end;
                vma.size = (vma_end - unmap_end) as usize;
                i += 1;
            } else if unmap_start > vma_start && unmap_end >= vma_end {
                // Trim the end of VMA
                vma.size = (unmap_start - vma_start) as usize;
                i += 1;
            } else {
                // Unmapping creates a hole in the middle: split into two VMAs
                let right_start = unmap_end;
                let right_size = (vma_end - unmap_end) as usize;
                let right_prot = vma.prot;
                let right_flags = vma.flags;
                let right_is_guard = vma.is_guard_page;
                let right_name = vma.name;

                // Adjust left side
                vma.size = (unmap_start - vma_start) as usize;

                // Create right side
                let mut right_vma = VmArea::new(
                    right_start,
                    right_size,
                    right_prot,
                    right_flags,
                    right_is_guard,
                    right_name,
                );

                // Move populated pages belonging to right side
                let mut right_pages = Vec::new();
                for (&page_addr, &phys) in vma.populated_pages.iter() {
                    if page_addr >= right_start && page_addr < vma_end {
                        right_pages.push((page_addr, phys));
                    }
                }
                for (p, phys) in right_pages {
                    vma.populated_pages.remove(&p);
                    right_vma.populated_pages.insert(p, phys);
                }

                self.vmas.insert(i + 1, right_vma);
                i += 2;
            }
        }

        Ok(())
    }

    /// Demand paging fault resolver called from the Page Fault (#PF) ISR.
    /// Returns true if the fault was on a valid lazy VMA and a frame was successfully mapped.
    pub fn handle_demand_fault(&mut self, fault_addr: u64, is_write: bool) -> bool {
        let vma_idx = match self.vmas.iter().position(|vma| vma.contains(fault_addr)) {
            Some(idx) => idx,
            None => return false,
        };

        let vma = &self.vmas[vma_idx];

        // Guard pages and PROT_NONE pages must trap!
        if vma.is_guard_page || vma.prot == PROT_NONE {
            return false;
        }

        // Permission check: write attempted on non-writable VMA
        if is_write && !vma.is_writable() {
            return false;
        }

        let base_page = align_down_page(fault_addr);

        // Allocate a new physical frame from PMM
        let phys = match crate::memory::pmm::allocate_frame() {
            Some(p) => p,
            None => return false,
        };

        // Zero the frame before mapping
        unsafe {
            (phys.as_u64() as *mut u8).write_bytes(0, PAGE_SIZE);
        }
        self.allocated_frames.push(phys.as_u64());

        let writable = vma.is_writable();
        if !self.map_user_page(VirtAddr(base_page), phys, writable) {
            let _ = crate::memory::pmm::free_frame(phys);
            return false;
        }

        // Invalidate TLB for the new page
        crate::memory::paging::flush_tlb_page(VirtAddr(base_page));

        // Record in populated pages map
        self.vmas[vma_idx].populated_pages.insert(base_page, phys.as_u64());

        crate::serial_println!(
            "[DemandPaging] Resolved #PF at Virt={:#x} -> Phys={:#x} (VMA: '{}')",
            base_page,
            phys.as_u64(),
            self.vmas[vma_idx].name
        );

        true
    }


    /// Dynamically expands or shrinks the process's heap (POSIX brk).
    /// Physical frames are NOT allocated immediately; they are demand-paged
    /// on first access (#PF).
    pub fn brk(&mut self, new_brk: u64) -> u64 {
        if new_brk == 0 || new_brk < self.heap_start {
            return self.heap_end;
        }

        if new_brk == self.heap_end {
            return self.heap_end;
        }

        // Limit heap growth to below MMAP_BASE_START (0x6000_0000)
        if new_brk > MMAP_BASE_START {
            return self.heap_end;
        }

        if new_brk > self.heap_end {
            // Expansion
            let new_aligned_end = align_up_page(new_brk);
            let heap_start = self.heap_start;

            // Check if there is already a "[heap]" VMA
            if let Some(pos) = self.vmas.iter().position(|v| v.name == "[heap]") {
                let current_start = self.vmas[pos].start;
                let new_size = (new_aligned_end - current_start) as usize;
                // Check if extending overlaps another VMA
                let overlaps_other = self.vmas.iter().enumerate().any(|(i, v)| {
                    i != pos && v.overlaps(current_start, new_size)
                });
                if overlaps_other {
                    return self.heap_end;
                }
                self.vmas[pos].size = new_size;
            } else {
                let size = (new_aligned_end - heap_start) as usize;
                // Check overlap with existing VMAs
                for existing in &self.vmas {
                    if existing.overlaps(heap_start, size) {
                        return self.heap_end;
                    }
                }
                let vma = VmArea::new(
                    heap_start,
                    size,
                    PROT_READ | PROT_WRITE,
                    MAP_PRIVATE | MAP_ANONYMOUS,
                    false,
                    "[heap]",
                );
                self.vmas.push(vma);
            }

            self.heap_end = new_brk;
            self.heap_end
        } else {
            // Shrinking
            let new_aligned_end = align_up_page(new_brk);
            let old_aligned_end = align_up_page(self.heap_end);

            if new_aligned_end < old_aligned_end {
                let unmap_len = (old_aligned_end - new_aligned_end) as usize;
                let _ = self.munmap(new_aligned_end, unmap_len);
            }

            if let Some(vma) = self.vmas.iter_mut().find(|v| v.name == "[heap]") {
                vma.size = (new_aligned_end - vma.start) as usize;
            }

            self.heap_end = new_brk;
            self.heap_end
        }
    }

    /// Computes the total virtual memory allocated across all active VMAs (in bytes).
    pub fn virtual_memory_size(&self) -> usize {
        self.vmas.iter().map(|v| v.size).sum()
    }

    /// Computes the total physical memory currently populated across all VMAs (in bytes).
    pub fn populated_memory_size(&self) -> usize {
        self.vmas.iter().map(|v| v.populated_pages.len() * PAGE_SIZE).sum()
    }

    /// Returns the number of physical frames currently owned by this AddressSpace.
    pub fn allocated_frame_count(&self) -> usize {
        self.allocated_frames.len()
    }

    /// Returns the base physical address of this address space's PML4 root table.
    pub fn cr3(&self) -> u64 {
        self.pml4_phys.as_u64()
    }

    /// Activates this address space in the CPU's CR3 control register.
    #[allow(dead_code)]
    pub unsafe fn activate(&self) {
        unsafe {
            write_cr3(self.pml4_phys);
        }
    }

    /// Restores the kernel's original address space in CR3.
    #[allow(dead_code)]
    pub unsafe fn deactivate(&self) {
        let kernel_cr3 = KERNEL_CR3.load(Ordering::SeqCst);
        if kernel_cr3 != 0 {
            unsafe {
                write_cr3(PhysAddr(kernel_cr3));
            }
        }
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        // Free every frame (page tables + user pages) back to the PMM.
        let count = self.allocated_frames.len();
        for &addr in &self.allocated_frames {
            let _ = crate::memory::pmm::free_frame(PhysAddr(addr));
        }
        crate::serial_println!(
            "[AddressSpace] Teardown complete: freed {} frames back to PMM (CR3: {:#x})",
            count, self.pml4_phys.as_u64()
        );
    }
}
