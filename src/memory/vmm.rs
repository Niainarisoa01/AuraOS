//! ============================================================================
//! Virtual Memory Areas (VMA) & Virtual Memory Management (VMM)
//! ============================================================================
//!
//! Provides data structures and abstractions for managing dynamic per-process
//! address spaces:
//! - Virtual Memory Areas (`VmArea`) tracking virtual ranges, access permissions,
//!   backing physical frames, and guard page attributes.
//! - Standard POSIX protection flags (`PROT_READ`, `PROT_WRITE`, `PROT_EXEC`, `PROT_NONE`).
//! - Standard POSIX mapping flags (`MAP_PRIVATE`, `MAP_SHARED`, `MAP_ANONYMOUS`,
//!   `MAP_FIXED`, `MAP_POPULATE`).
//! - Page-aligned address calculations, overlap detection, and lazy Demand Paging.

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use crate::memory::paging::PAGE_SIZE;

// ============================================================================
// Protection & Mapping Constants (POSIX / Linux Compatible)
// ============================================================================

/// Page can not be accessed (e.g. guard page).
pub const PROT_NONE: u32 = 0x0;
/// Page can be read.
pub const PROT_READ: u32 = 0x1;
/// Page can be written.
pub const PROT_WRITE: u32 = 0x2;
/// Page can be executed.
pub const PROT_EXEC: u32 = 0x4;

/// Changes are shared among processes.
pub const MAP_SHARED: u32 = 0x01;
/// Changes are private to this process (Copy-on-Write).
pub const MAP_PRIVATE: u32 = 0x02;
/// Interpret address exactly; do not select alternative.
pub const MAP_FIXED: u32 = 0x10;
/// Don't use a file; allocate anonymous zero-filled memory.
pub const MAP_ANONYMOUS: u32 = 0x20;
/// Alias for MAP_ANONYMOUS.
pub const MAP_ANON: u32 = MAP_ANONYMOUS;
/// Populate (prefault) page tables immediately.
pub const MAP_POPULATE: u32 = 0x8000;

/// Default virtual memory bounds for dynamic user `mmap` allocations.
/// Placed between 1.5 GiB and below user stack (2 GiB).
pub const MMAP_BASE_START: u64 = 0x0000_0000_6000_0000; // 1.5 GiB
pub const MMAP_BASE_END: u64   = 0x0000_0000_7F00_0000; // 2 GiB - 16 MiB

/// Virtual Memory Management error conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmmError {
    OutOfMemory,
    InvalidAddress,
    InvalidArguments,
    Overlap,
    PermissionDenied,
    NotFound,
}

impl VmmError {
    pub fn as_str(&self) -> &'static str {
        match self {
            VmmError::OutOfMemory => "Out of physical or virtual memory",
            VmmError::InvalidAddress => "Invalid or unaligned memory address",
            VmmError::InvalidArguments => "Invalid arguments supplied to VMM operation",
            VmmError::Overlap => "Requested virtual range overlaps existing mapping",
            VmmError::PermissionDenied => "Access violation: permission denied",
            VmmError::NotFound => "No virtual memory area found matching address",
        }
    }
}

/// A contiguous virtual memory area (VMA) with uniform protection and mapping flags.
#[derive(Clone, Debug)]
pub struct VmArea {
    /// Virtual base address (must be page-aligned to 4 KiB).
    pub start: u64,
    /// Total byte size of the area (must be a positive multiple of PAGE_SIZE).
    pub size: usize,
    /// Protection flags (PROT_READ, PROT_WRITE, PROT_EXEC, PROT_NONE).
    pub prot: u32,
    /// Mapping flags (MAP_PRIVATE, MAP_ANONYMOUS, etc.).
    pub flags: u32,
    /// True if this area acts as a guard page (traps on access to catch stack overflows).
    pub is_guard_page: bool,
    /// Human-readable label for debugging and /proc maps inspection.
    pub name: &'static str,
    /// Mapping of populated virtual pages to physical frame addresses:
    /// (virtual_page_base -> physical_frame_address).
    pub populated_pages: BTreeMap<u64, u64>,
}

impl VmArea {
    /// Creates a new Virtual Memory Area with the specified properties.
    pub fn new(
        start: u64,
        size: usize,
        prot: u32,
        flags: u32,
        is_guard_page: bool,
        name: &'static str,
    ) -> Self {
        Self {
            start,
            size,
            prot,
            flags,
            is_guard_page,
            name,
            populated_pages: BTreeMap::new(),
        }
    }

    /// Returns the non-inclusive ending virtual address of this area.
    #[inline]
    pub fn end(&self) -> u64 {
        self.start + self.size as u64
    }

    /// Checks whether a virtual address falls within this virtual memory area.
    #[inline]
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end()
    }

    /// Checks whether this area overlaps with the given virtual address range.
    #[inline]
    pub fn overlaps(&self, start: u64, size: usize) -> bool {
        let end = start + size as u64;
        !(end <= self.start || start >= self.end())
    }

    /// Returns whether this VMA allows read operations.
    #[inline]
    pub fn is_readable(&self) -> bool {
        (self.prot & PROT_READ) != 0
    }

    /// Returns whether this VMA allows write operations.
    #[inline]
    pub fn is_writable(&self) -> bool {
        (self.prot & PROT_WRITE) != 0
    }

    /// Returns whether this VMA allows code execution.
    #[inline]
    pub fn is_executable(&self) -> bool {
        (self.prot & PROT_EXEC) != 0
    }
}

/// Helper function to align a virtual address up to the nearest 4 KiB page boundary.
#[inline]
pub fn align_up_page(addr: u64) -> u64 {
    (addr + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1)
}

/// Helper function to align a virtual address down to the nearest 4 KiB page boundary.
#[inline]
pub fn align_down_page(addr: u64) -> u64 {
    addr & !(PAGE_SIZE as u64 - 1)
}
