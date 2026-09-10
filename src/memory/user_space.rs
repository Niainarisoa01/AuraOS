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
//! All page table entries must use physical addresses obtained via `virt_to_phys()`.

use alloc::alloc::{alloc_zeroed, dealloc, Layout};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::memory::paging::{
    read_cr3, write_cr3, virt_to_phys, PhysAddr, VirtAddr, PageTable, page_flags, PAGE_SIZE,
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
/// Returns both the virtual pointer (for kernel read/write) and the
/// physical address (for use in page table entries and CR3).
unsafe fn alloc_page_table() -> Option<PageTableAlloc> {
    let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;
    let ptr = unsafe { alloc_zeroed(layout) as *mut PageTable };
    if ptr.is_null() {
        return None;
    }
    // Translate the virtual address to physical using the active page tables
    let phys = virt_to_phys(ptr as u64)?;
    Some(PageTableAlloc {
        virt: ptr,
        phys: PhysAddr(phys),
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

/// Tracks a page table mapping between physical frame and virtual pointer.
#[derive(Clone, Copy)]
pub struct AllocatedPageTable {
    pub phys: u64,
    pub virt: *mut PageTable,
}

/// Represents an isolated virtual memory address space for a user process.
#[allow(dead_code)]
pub struct AddressSpace {
    /// Virtual pointer to the PML4 table (for kernel read/write access)
    pml4_virt: *mut PageTable,
    /// Physical address of the PML4 table (for CR3 and CPU page walks)
    pml4_phys: PhysAddr,
    /// Tracked page tables (phys -> virt mapping)
    tables: Vec<AllocatedPageTable>,
    /// Memory blocks allocated for page tables and user pages (virtual addrs for deallocation)
    allocated_frames: Vec<*mut u8>,
}

impl AddressSpace {
    /// Creates a new isolated address space with kernel mappings preserved.
    pub fn new() -> Option<Self> {
        let kcr3 = KERNEL_CR3.load(Ordering::Relaxed);
        let kernel_cr3 = if kcr3 != 0 { PhysAddr(kcr3) } else { read_cr3() };
        let kernel_pml4 = kernel_cr3.as_u64() as *const PageTable;

        let pml4 = unsafe { alloc_page_table()? };
        let mut allocated_frames = Vec::new();
        let mut tables = Vec::new();

        tables.push(AllocatedPageTable {
            phys: pml4.phys.as_u64(),
            virt: pml4.virt,
        });
        allocated_frames.push(pml4.virt as *mut u8);

        let pml4_virt = pml4.virt;
        let pml4_phys = pml4.phys;

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
                tables.push(AllocatedPageTable {
                    phys: user_pdpt.phys.as_u64(),
                    virt: user_pdpt.virt,
                });
                allocated_frames.push(user_pdpt.virt as *mut u8);

                // Copy all kernel PDPT entries
                for i in 0..512 {
                    (*user_pdpt.virt).entries[i] = (*kernel_pdpt).entries[i];
                }

                // Point PML4[0] to the new user PDPT using its PHYSICAL address
                (*pml4_virt).entries[0].set(
                    user_pdpt.phys,
                    page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER_ACCESSIBLE,
                );

                crate::serial_println!(
                    "[UserSpace] PML4 virt={:#x} phys={:#x}, PDPT virt={:#x} phys={:#x}",
                    pml4_virt as u64, pml4_phys.as_u64(),
                    user_pdpt.virt as u64, user_pdpt.phys.as_u64()
                );
            }
        }

        Some(AddressSpace {
            pml4_virt,
            pml4_phys,
            tables,
            allocated_frames,
        })
    }

    /// Returns the virtual address corresponding to a physical address of a page table.
    /// If allocated by this AddressSpace, returns its virtual pointer.
    /// Otherwise falls back to identity mapping (for bootloader low tables).
    fn phys_to_virt(&self, phys: u64) -> *mut PageTable {
        for t in &self.tables {
            if t.phys == phys {
                return t.virt;
            }
        }
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
                self.tables.push(AllocatedPageTable {
                    phys: new_table.phys.as_u64(),
                    virt: new_table.virt,
                });
                self.allocated_frames.push(new_table.virt as *mut u8);
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
                self.tables.push(AllocatedPageTable {
                    phys: new_table.phys.as_u64(),
                    virt: new_table.virt,
                });
                self.allocated_frames.push(new_table.virt as *mut u8);
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
                self.tables.push(AllocatedPageTable {
                    phys: new_table.phys.as_u64(),
                    virt: new_table.virt,
                });
                self.allocated_frames.push(new_table.virt as *mut u8);
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

    /// Allocates a new physical page from the heap and maps it into user space.
    /// The physical address is obtained via virt_to_phys() translation.
    pub fn allocate_and_map_page(&mut self, virt: VirtAddr, writable: bool) -> Option<*mut u8> {
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;
        let frame_ptr = unsafe { alloc_zeroed(layout) };
        if frame_ptr.is_null() {
            return None;
        }

        self.allocated_frames.push(frame_ptr);
        // Get the REAL physical address (not virtual!)
        let phys_addr = virt_to_phys(frame_ptr as u64)?;
        let phys = PhysAddr(phys_addr);

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

    /// Allocates a user-space stack of `num_pages` pages ending at `top_virt`.
    pub fn allocate_user_stack(&mut self, top_virt: VirtAddr, num_pages: usize) -> bool {
        for i in 1..=num_pages {
            let page_virt = VirtAddr(top_virt.as_u64() - (i * PAGE_SIZE) as u64);
            if self.allocate_and_map_page(page_virt, true).is_none() {
                return false;
            }
        }
        true
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
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
        for &frame in &self.allocated_frames {
            unsafe {
                dealloc(frame, layout);
            }
        }
    }
}
