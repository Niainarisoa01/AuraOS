#![allow(dead_code)]

//! ============================================================================
//! Memory Management — Physical/Virtual Addresses & x86_64 Paging
//! ============================================================================
//!
//! Provides abstractions for 64-bit virtual and physical memory addresses,
//! 4-level page table representations, and CR3 control register manipulation.

/// Standard x86_64 physical and virtual page size (4 KiB = 4096 bytes).
pub const PAGE_SIZE: usize = 4096;

/// Represents a 64-bit physical memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(pub u64);

impl PhysAddr {
    /// Returns the raw 64-bit integer representation.
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Checks whether the address is aligned to the given alignment boundary.
    #[inline]
    pub const fn is_aligned_to(&self, align: u64) -> bool {
        (self.0 & (align - 1)) == 0
    }

    /// Rounds the address up to the nearest alignment boundary.
    #[inline]
    pub const fn align_up(&self, align: u64) -> PhysAddr {
        PhysAddr((self.0 + align - 1) & !(align - 1))
    }

    /// Rounds the address down to the nearest alignment boundary.
    #[inline]
    pub const fn align_down(&self, align: u64) -> PhysAddr {
        PhysAddr(self.0 & !(align - 1))
    }
}

/// Represents a 64-bit canonical virtual memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(pub u64);

impl VirtAddr {
    /// Returns the raw 64-bit integer representation.
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Casts the virtual address to a constant raw pointer.
    #[inline]
    pub const fn as_ptr<T>(&self) -> *const T {
        self.0 as *const T
    }

    /// Casts the virtual address to a mutable raw pointer.
    #[inline]
    pub const fn as_mut_ptr<T>(&self) -> *mut T {
        self.0 as *mut T
    }

    /// Returns the Level 4 Page Map (PML4) index (bits 39..47).
    #[inline]
    pub const fn p4_index(&self) -> usize {
        ((self.0 >> 39) & 0o777) as usize
    }

    /// Returns the Level 3 Page Directory Pointer (PDPT) index (bits 30..38).
    #[inline]
    pub const fn p3_index(&self) -> usize {
        ((self.0 >> 30) & 0o777) as usize
    }

    /// Returns the Level 2 Page Directory (PD) index (bits 21..29).
    #[inline]
    pub const fn p2_index(&self) -> usize {
        ((self.0 >> 21) & 0o777) as usize
    }

    /// Returns the Level 1 Page Table (PT) index (bits 12..20).
    #[inline]
    pub const fn p1_index(&self) -> usize {
        ((self.0 >> 12) & 0o777) as usize
    }

    /// Returns the 12-bit offset within the 4 KiB physical page.
    #[inline]
    pub const fn page_offset(&self) -> usize {
        (self.0 & 0xFFF) as usize
    }
}

// ============================================================================
// x86_64 Page Table Flags & Entries
// ============================================================================

pub mod page_flags {
    pub const PRESENT: u64 = 1 << 0;          // Page is currently in physical memory
    pub const WRITABLE: u64 = 1 << 1;         // Read/Write access permitted
    pub const USER_ACCESSIBLE: u64 = 1 << 2;  // Accessible from Ring 3 User Space
    pub const WRITE_THROUGH: u64 = 1 << 3;    // Write-through caching enabled
    pub const NO_CACHE: u64 = 1 << 4;         // Page cache disabled
    pub const ACCESSED: u64 = 1 << 5;         // Set by CPU when the page is accessed
    pub const DIRTY: u64 = 1 << 6;            // Set by CPU when the page is written to
    pub const HUGE_PAGE: u64 = 1 << 7;        // 2 MiB or 1 GiB page size
    pub const GLOBAL: u64 = 1 << 8;           // Page not flushed from TLB on CR3 reload
    pub const NO_EXECUTE: u64 = 1 << 63;      // Instruction fetching disabled (NX bit)
}

/// A single 64-bit entry in an x86_64 page table.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(pub u64);

impl PageTableEntry {
    /// Creates an unused, zeroed page table entry.
    pub const fn new() -> Self {
        PageTableEntry(0)
    }

    /// Checks if the page is marked present.
    #[inline]
    pub const fn is_present(&self) -> bool {
        (self.0 & page_flags::PRESENT) != 0
    }

    /// Extracts the physical frame address pointed to by this entry.
    #[inline]
    pub const fn addr(&self) -> PhysAddr {
        // Physical frame address lives in bits 12..51
        PhysAddr(self.0 & 0x000F_FFFF_FFFF_F000)
    }

    /// Returns the raw flag bits of this entry.
    #[inline]
    pub const fn flags(&self) -> u64 {
        self.0 & 0xFFF0_0000_0000_0FFF
    }

    /// Configures the entry with a physical address and permission flags.
    #[inline]
    pub fn set(&mut self, addr: PhysAddr, flags: u64) {
        self.0 = (addr.as_u64() & 0x000F_FFFF_FFFF_F000) | (flags & 0xFFF0_0000_0000_0FFF);
    }

    /// Clears the entry (marks it not present and unused).
    #[inline]
    pub fn set_unused(&mut self) {
        self.0 = 0;
    }
}

/// Representation of a 512-entry x86_64 page table (4096 bytes, 4 KiB aligned).
#[repr(align(4096))]
#[repr(C)]
pub struct PageTable {
    pub entries: [PageTableEntry; 512],
}

impl PageTable {
    /// Creates an empty page table with all 512 entries zeroed out.
    pub const fn new() -> Self {
        PageTable {
            entries: [PageTableEntry::new(); 512],
        }
    }

    /// Clears all entries in the page table.
    pub fn zero(&mut self) {
        for entry in self.entries.iter_mut() {
            entry.set_unused();
        }
    }
}

/// Reads the base physical address of the active Level 4 Page Table from the CR3 register.
pub fn read_cr3() -> PhysAddr {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr3",
            out(reg) value,
            options(nomem, nostack, preserves_flags)
        );
    }
    PhysAddr(value & 0x000F_FFFF_FFFF_F000)
}

/// Maps the 3 GiB .. 4 GiB PCI MMIO range (0xC000_0000..0xFFFF_FFFF)
/// as a 1 GiB huge page in the active page table.
///
/// This provides direct virtual-to-physical identity access to the
/// Bochs VGA linear framebuffer (at 0xFD000000) and PCI MMIO registers.
pub fn map_mmio_pci_range() {
    let cr3 = read_cr3();
    let p4_ptr = cr3.as_u64() as *const u64;
    unsafe {
        let p4_entry0 = *p4_ptr;
        if (p4_entry0 & page_flags::PRESENT) != 0 {
            let p3_phys = p4_entry0 & 0x000F_FFFF_FFFF_F000;
            // P3 is identity-mapped in the low memory region (0x1000..0x14000)
            let p3_entry3_ptr = (p3_phys + 3 * 8) as *mut u64;
            // 0xC000_0000 | PRESENT (1) | WRITABLE (2) | HUGE_PAGE (0x80)
            *p3_entry3_ptr = 0xC000_0000 | page_flags::PRESENT | page_flags::WRITABLE | page_flags::HUGE_PAGE;

            // Reload CR3 to flush the CPU Translation Lookaside Buffer (TLB)
            flush_tlb();
        }
    }
}

/// Writes a new physical address to the CR3 control register, switching the active page table.
#[inline]
pub unsafe fn write_cr3(pml4_phys: PhysAddr) {
    unsafe {
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) pml4_phys.as_u64(),
            options(nostack, preserves_flags)
        );
    }
}

/// Flushes the Translation Lookaside Buffer (TLB) by reloading CR3.
#[inline]
pub fn flush_tlb() {
    unsafe {
        core::arch::asm!(
            "mov rax, cr3",
            "mov cr3, rax",
            out("rax") _,
            options(nostack, preserves_flags)
        );
    }
}

/// Translates a kernel virtual address to its physical address by walking
/// the currently active page tables.
///
/// This is critical because the bootloader does NOT identity-map the kernel:
/// e.g. virtual 0x100000 may map to physical 0x401000. Heap allocations
/// are similarly offset, so `ptr as u64` is NOT the physical address.
///
/// The bootloader's page table structures (PML4, PDPT, PD, PT) are stored
/// in low physical memory (~0x1000..0x5000) which IS identity-mapped,
/// allowing us to dereference those physical addresses as virtual pointers.
pub fn virt_to_phys(virt: u64) -> Option<u64> {
    let cr3 = read_cr3().as_u64();
    let p4_idx = ((virt >> 39) & 0x1FF) as usize;
    let p3_idx = ((virt >> 30) & 0x1FF) as usize;
    let p2_idx = ((virt >> 21) & 0x1FF) as usize;
    let p1_idx = ((virt >> 12) & 0x1FF) as usize;

    unsafe {
        // Level 4: PML4 (at physical = virtual address, identity-mapped by bootloader)
        let pml4 = cr3 as *const PageTable;
        let p4e = (*pml4).entries[p4_idx];
        if !p4e.is_present() { return None; }

        // Level 3: PDPT
        let pdpt_phys = p4e.addr().as_u64();
        let pdpt = pdpt_phys as *const PageTable;
        let p3e = (*pdpt).entries[p3_idx];
        if !p3e.is_present() { return None; }
        if (p3e.flags() & page_flags::HUGE_PAGE) != 0 {
            // 1 GiB huge page
            return Some(p3e.addr().as_u64() | (virt & 0x3FFF_FFFF));
        }

        // Level 2: Page Directory
        let pd_phys = p3e.addr().as_u64();
        let pd = pd_phys as *const PageTable;
        let p2e = (*pd).entries[p2_idx];
        if !p2e.is_present() { return None; }
        if (p2e.flags() & page_flags::HUGE_PAGE) != 0 {
            // 2 MiB huge page
            return Some(p2e.addr().as_u64() | (virt & 0x1F_FFFF));
        }

        // Level 1: Page Table
        let pt_phys = p2e.addr().as_u64();
        let pt = pt_phys as *const PageTable;
        let p1e = (*pt).entries[p1_idx];
        if !p1e.is_present() { return None; }

        Some(p1e.addr().as_u64() | (virt & 0xFFF))
    }
}
