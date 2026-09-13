//! ============================================================================
//! BIOS BootInfo Adapter
//! ============================================================================
//!
//! Translates `bootloader::bootinfo::BootInfo` into the unified `BootInfo` structure.

use core::cell::UnsafeCell;
use super::{BootInfo, BootMethod, MemoryRegion, MemoryRegionKind};

/// Capacity for static converted memory regions.
const MAX_BIOS_REGIONS: usize = 128;

struct SyncStorage<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncStorage<T> {}

static BIOS_REGIONS: SyncStorage<[MemoryRegion; MAX_BIOS_REGIONS]> = SyncStorage(UnsafeCell::new([MemoryRegion {
    start: 0,
    end: 0,
    kind: MemoryRegionKind::Reserved,
}; MAX_BIOS_REGIONS]));

static BIOS_BOOT_INFO: SyncStorage<Option<BootInfo>> = SyncStorage(UnsafeCell::new(None));

/// Converts a legacy BIOS `bootloader::bootinfo::BootInfo` into a static reference to unified `BootInfo`.
pub fn convert_bios_boot_info(bios: &'static bootloader::bootinfo::BootInfo) -> &'static BootInfo {
    let regions_ptr = BIOS_REGIONS.0.get();
    let mut count = 0usize;

    for r in bios.memory_map.iter() {
        if count >= MAX_BIOS_REGIONS {
            break;
        }
        let kind = match r.region_type {
            bootloader::bootinfo::MemoryRegionType::Usable => MemoryRegionKind::Usable,
            bootloader::bootinfo::MemoryRegionType::Reserved => MemoryRegionKind::Reserved,
            bootloader::bootinfo::MemoryRegionType::AcpiReclaimable => MemoryRegionKind::AcpiReclaimable,
            bootloader::bootinfo::MemoryRegionType::AcpiNvs => MemoryRegionKind::AcpiNvs,
            bootloader::bootinfo::MemoryRegionType::BadMemory => MemoryRegionKind::BadMemory,
            bootloader::bootinfo::MemoryRegionType::Bootloader => MemoryRegionKind::Bootloader,
            bootloader::bootinfo::MemoryRegionType::Kernel => MemoryRegionKind::Kernel,
            bootloader::bootinfo::MemoryRegionType::KernelStack => MemoryRegionKind::Kernel,
            bootloader::bootinfo::MemoryRegionType::PageTable => MemoryRegionKind::Bootloader,
            _ => MemoryRegionKind::Reserved,
        };

        unsafe {
            (*regions_ptr)[count] = MemoryRegion {
                start: r.range.start_addr(),
                end: r.range.end_addr(),
                kind,
            };
        }
        count += 1;
    }

    let static_slice: &'static [MemoryRegion] = unsafe {
        core::slice::from_raw_parts((*regions_ptr).as_ptr(), count)
    };

    let info = BootInfo {
        magic: super::BOOT_INFO_MAGIC,
        memory_map: static_slice,
        acpi_rsdp: None, // Discovered via EBDA/BIOS ROM memory scan in BIOS mode
        boot_method: BootMethod::Bios,
    };

    let info_ptr = BIOS_BOOT_INFO.0.get();
    unsafe {
        *info_ptr = Some(info);
        (*info_ptr).as_ref().unwrap()
    }
}
