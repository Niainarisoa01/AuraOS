//! ============================================================================
//! Unified Boot Architecture — BIOS / UEFI Abstraction
//! ============================================================================
//!
//! Provides bootloader-independent data structures passed to `kernel_main`:
//! - `BootInfo` (unified memory map, optional ACPI RSDP, boot method)
//! - `MemoryRegion` and `MemoryRegionKind`
//! - `BootMethod` (`Bios`, `Uefi`)

pub mod bios;

use crate::sync::Spinlock;

/// Method used to boot AuraOS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootMethod {
    Bios,
    Uefi,
}

/// Category of physical memory region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryRegionKind {
    /// Usable physical RAM available for general allocation.
    Usable,
    /// Reserved by hardware, firmware, or memory holes.
    Reserved,
    /// ACPI reclaimable memory (tables that can be reclaimed after ACPI init).
    AcpiReclaimable,
    /// ACPI Non-Volatile Storage (must be preserved across sleep/wake).
    AcpiNvs,
    /// Defective memory reported bad by hardware/firmware.
    BadMemory,
    /// Memory used by the bootloader itself.
    Bootloader,
    /// Memory occupied by the kernel executable image (.text, .rodata, .data, .bss).
    Kernel,
    /// Memory mapped I/O space.
    Mmio,
}

/// A contiguous physical memory region described in real physical bytes.
#[derive(Clone, Copy, Debug)]
pub struct MemoryRegion {
    /// Physical start address (inclusive).
    pub start: u64,
    /// Physical end address (exclusive).
    pub end: u64,
    /// Type/Classification of the memory region.
    pub kind: MemoryRegionKind,
}

impl MemoryRegion {
    /// Returns the length of this region in bytes.
    #[inline]
    pub const fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    /// Returns true if this region is considered usable physical RAM by the PMM.
    #[inline]
    pub const fn is_usable(&self) -> bool {
        matches!(self.kind, MemoryRegionKind::Usable)
    }
}

/// Magic identifier ("AURA_OS!") verifying a valid unified BootInfo structure.
pub const BOOT_INFO_MAGIC: u64 = 0x41555241_5F4F5321;

/// Unified boot information passed from the bootloader to `kernel_main`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BootInfo {
    /// Magic identifier for runtime validation.
    pub magic: u64,
    /// List of physical memory regions (ordered, non-overlapping).
    pub memory_map: &'static [MemoryRegion],
    /// Physical address of the ACPI Root System Description Pointer (RSDP), if discovered.
    pub acpi_rsdp: Option<u64>,
    /// Whether AuraOS was booted via legacy BIOS or native UEFI.
    pub boot_method: BootMethod,
}

/// Global static snapshot of the active BootInfo.
static ACTIVE_BOOT_INFO: Spinlock<Option<BootInfo>> = Spinlock::new(None);

/// Registers the active BootInfo during early kernel initialization.
pub fn set_active_boot_info(info: BootInfo) {
    *ACTIVE_BOOT_INFO.lock() = Some(info);
}

/// Returns a copy of the active BootInfo if initialized.
pub fn get_boot_info() -> Option<BootInfo> {
    *ACTIVE_BOOT_INFO.lock()
}

/// Returns the active boot method (Bios or Uefi).
pub fn current_boot_method() -> BootMethod {
    get_boot_info().map(|b| b.boot_method).unwrap_or(BootMethod::Bios)
}
