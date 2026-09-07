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
