//! ============================================================================
//! AuraOS UEFI Bootloader — Handcrafted Native 64-bit EFI Application
//! ============================================================================
//!
//! Written in pure `no_std` Rust with zero external dependencies.
//! Responsibilities:
//! 1. Receives system control from UEFI firmware via `efi_main`.
//! 2. Locates ACPI 2.0 / 1.0 RSDP in the UEFI Configuration Table.
//! 3. Loads the 64-bit ELF kernel image into physical RAM at 1 MiB (0x100000).
//! 4. Builds a clean identity-mapped 4-level page table matching kernel paging topology.
//! 5. Obtains the firmware physical memory map and exits UEFI Boot Services.
//! 6. Converts the UEFI memory descriptor map into the unified `MemoryRegion` format.
//! 7. Transfers control to `kernel_main` with `%rdi` pointing to `BootInfo`.

#![no_std]
#![no_main]

mod uefi;
mod elf;

use core::panic::PanicInfo;
use uefi::*;

/// Magic identifier matching `src/boot/mod.rs` ("AURA_OS!").
pub const BOOT_INFO_MAGIC: u64 = 0x41555241_5F4F5321;

/// Boot method enum (matches kernel definition).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootMethod {
    Bios = 0,
    Uefi = 1,
}

/// Category of physical memory region (matches kernel definition).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryRegionKind {
    Usable = 0,
    Reserved = 1,
    AcpiReclaimable = 2,
    AcpiNvs = 3,
    BadMemory = 4,
    Bootloader = 5,
    Kernel = 6,
    Mmio = 7,
}

/// A contiguous physical memory region (matches kernel definition).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub kind: MemoryRegionKind,
}

/// Unified boot information passed to `kernel_main`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BootInfo {
    pub magic: u64,
    pub memory_map: &'static [MemoryRegion],
    pub acpi_rsdp: Option<u64>,
    pub boot_method: BootMethod,
}

/// Thread-safe wrapper for interior mutability in no_std bare-metal statics.
struct SyncStorage<T>(core::cell::UnsafeCell<T>);
unsafe impl<T> Sync for SyncStorage<T> {}

/// Static buffer holding converted memory regions for the kernel.
static KERNEL_REGIONS: SyncStorage<[MemoryRegion; 256]> = SyncStorage(core::cell::UnsafeCell::new([MemoryRegion {
    start: 0,
    end: 0,
    kind: MemoryRegionKind::Reserved,
}; 256]));

/// Static BootInfo instance passed to `kernel_main`.
static KERNEL_BOOT_INFO: SyncStorage<Option<BootInfo>> = SyncStorage(core::cell::UnsafeCell::new(None));

/// Embedded kernel ELF binary built from `aura-kernel`.
static KERNEL_ELF: &[u8] = include_bytes!("../../../target/x86_64-unknown-none/release/aura-kernel");

/// UTF-8 to UCS-2 console printing helper for UEFI Simple Text Output.
fn print_str(con_out: *mut EfiSimpleTextOutputProtocol, s: &str) {
    if con_out.is_null() {
        return;
    }
    let mut buf = [0u16; 128];
    let mut i = 0;
    for b in s.bytes() {
        if b == b'\n' {
            if i < buf.len() - 2 {
                buf[i] = b'\r' as u16;
                buf[i + 1] = b'\n' as u16;
                i += 2;
            }
        } else if i < buf.len() - 1 {
            buf[i] = b as u16;
            i += 1;
        }
        if i >= buf.len() - 2 {
            buf[i] = 0;
            unsafe {
                ((*con_out).output_string)(con_out, buf.as_ptr());
            }
            i = 0;
        }
    }
    if i > 0 {
        buf[i] = 0;
        unsafe {
            ((*con_out).output_string)(con_out, buf.as_ptr());
        }
    }
}

/// Primary UEFI Entry Point invoked by firmware.
#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(
    image_handle: EfiHandle,
    system_table: *const EfiSystemTable,
) -> EfiStatus {
    if system_table.is_null() {
        return EFI_LOAD_ERROR;
    }

    let st = unsafe { &*system_table };
    let con_out = st.con_out;
    let bs = match unsafe { st.boot_services.as_ref() } {
        Some(s) => s,
        None => return EFI_LOAD_ERROR,
    };

    print_str(con_out, "============================================================\n");
    print_str(con_out, "        AuraOS Native 64-bit UEFI Bootloader v0.1.0        \n");
    print_str(con_out, "============================================================\n");

    // 1. Locate ACPI 2.0 / 1.0 Root System Description Pointer (RSDP) in Configuration Table
    let mut acpi_rsdp: Option<u64> = None;
    let entries_count = st.number_of_table_entries;
    let config_tables = st.configuration_table;

    if !config_tables.is_null() {
        for i in 0..entries_count {
            let entry = unsafe { *config_tables.add(i) };
            if entry.vendor_guid == EFI_ACPI_20_TABLE_GUID {
                acpi_rsdp = Some(entry.vendor_table as u64);
                print_str(con_out, "[OK] ACPI 2.0+ RSDP located via EFI Configuration Table.\n");
                break;
            }
            if entry.vendor_guid == ACPI_10_TABLE_GUID && acpi_rsdp.is_none() {
                acpi_rsdp = Some(entry.vendor_table as u64);
                print_str(con_out, "[OK] ACPI 1.0 RSDP located via EFI Configuration Table.\n");
            }
        }
    }

    // 2. Allocate fixed physical RAM at 1 MiB (0x100000) for the kernel image.
    // The kernel binary has LOAD segments spanning from 1 MiB to ~10 MiB.
    let kernel_pages = 2560; // 10 MiB / 4 KiB
    let mut kernel_phys_addr = 0x0010_0000u64;
    let _ = unsafe {
        (bs.allocate_pages)(
            EfiAllocateType::AllocateAddress,
            EfiMemoryType::LoaderData,
            kernel_pages,
            &mut kernel_phys_addr,
        )
    };

    // 3. Parse and load the ELF binary into physical RAM
    print_str(con_out, "[OK] Parsing and deploying AuraOS kernel ELF image...\n");
    let entry_point = match elf::load_elf(KERNEL_ELF) {
        Ok(entry) => entry,
        Err(_err) => {
            print_str(con_out, "[ERR] Failed to parse/deploy kernel ELF.\n");
            return EFI_LOAD_ERROR;
        }
    };
    print_str(con_out, "[OK] Kernel segments deployed at 0x100000. Entry point verified.\n");

    // 4. Allocate 4 physical frames (16 KiB) for our identity-mapped page tables
    // PML4, PDPT, PD, PT topology covering 0..512 MiB and 3..4 GiB (MMIO)
    let mut page_tables_addr = 0u64;
    let pt_status = unsafe {
        (bs.allocate_pages)(
            EfiAllocateType::AllocateAnyPages,
            EfiMemoryType::RuntimeServicesData,
            4,
            &mut page_tables_addr,
        )
    };
    if pt_status != EFI_SUCCESS {
        print_str(con_out, "[ERR] Failed to allocate page tables.\n");
        return EFI_LOAD_ERROR;
    }

    // 5. Allocate 8 physical frames (32 KiB) for a dedicated kernel stack
    let mut stack_addr = 0u64;
    let stack_status = unsafe {
        (bs.allocate_pages)(
            EfiAllocateType::AllocateAnyPages,
            EfiMemoryType::RuntimeServicesData,
            8,
            &mut stack_addr,
        )
    };
    if stack_status != EFI_SUCCESS {
        print_str(con_out, "[ERR] Failed to allocate kernel stack.\n");
        return EFI_LOAD_ERROR;
    }
    let stack_top = stack_addr + (8 * 4096);

    // 6. Build the 4-level page table
    unsafe {
        let pml4 = page_tables_addr as *mut u64;
        let pdpt = (page_tables_addr + 0x1000) as *mut u64;
        let pd = (page_tables_addr + 0x2000) as *mut u64;
        let pt = (page_tables_addr + 0x3000) as *mut u64;

        core::ptr::write_bytes(pml4, 0, 512);
        core::ptr::write_bytes(pdpt, 0, 512);
        core::ptr::write_bytes(pd, 0, 512);
        core::ptr::write_bytes(pt, 0, 512);

        const PRESENT: u64 = 1 << 0;
        const WRITABLE: u64 = 1 << 1;
        const HUGE_PAGE: u64 = 1 << 7;

        // PML4[0] -> PDPT
        *pml4 = (pdpt as u64) | PRESENT | WRITABLE;

        // PDPT[0] -> PD
        *pdpt = (pd as u64) | PRESENT | WRITABLE;

        // PDPT[3] -> 1 GiB huge page at 3 GiB (0xC000_0000..0xFFFF_FFFF for MMIO/LAPIC)
        *pdpt.add(3) = 0xC000_0000 | PRESENT | WRITABLE | HUGE_PAGE;

        // PD[0] -> PT (maps first 2 MiB in 4 KiB pages)
        *pd = (pt as u64) | PRESENT | WRITABLE;

        // PT maps 0..2 MiB in 4 KiB pages
        for i in 0..512 {
            *pt.add(i) = ((i as u64) << 12) | PRESENT | WRITABLE;
        }

        // PD[1..256] maps 2 MiB..512 MiB using 2 MiB huge pages
        for j in 1..256 {
            *pd.add(j) = ((j as u64) << 21) | PRESENT | WRITABLE | HUGE_PAGE;
        }
    }

    // 7. Obtain UEFI physical memory map before ExitBootServices
    let mut memory_map_buffer = [0u8; 32768]; // 32 KiB buffer
    let mut map_size = memory_map_buffer.len();
    let mut map_key = 0usize;
    let mut descriptor_size = 0usize;
    let mut descriptor_version = 0u32;

    let map_status = unsafe {
        (bs.get_memory_map)(
            &mut map_size,
            memory_map_buffer.as_mut_ptr() as *mut EfiMemoryDescriptor,
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        )
    };
    if map_status != EFI_SUCCESS {
        print_str(con_out, "[ERR] Failed to retrieve UEFI memory map.\n");
        return EFI_LOAD_ERROR;
    }

    // 8. Exit UEFI Boot Services
    print_str(con_out, "[OK] Exiting UEFI Boot Services. Transferring control to AuraOS...\n");
    let exit_status = unsafe { (bs.exit_boot_services)(image_handle, map_key) };
    if exit_status != EFI_SUCCESS {
        // As per UEFI specification, if the map key changed, query again and retry once
        map_size = memory_map_buffer.len();
        let _ = unsafe {
            (bs.get_memory_map)(
                &mut map_size,
                memory_map_buffer.as_mut_ptr() as *mut EfiMemoryDescriptor,
                &mut map_key,
                &mut descriptor_size,
                &mut descriptor_version,
            )
        };
        let second_try = unsafe { (bs.exit_boot_services)(image_handle, map_key) };
        if second_try != EFI_SUCCESS {
            return EFI_LOAD_ERROR;
        }
    }

    // =========================================================================
    // BARE-METAL EXECUTION — UEFI Firmware is now offline
    // =========================================================================

    // Disable interrupts immediately
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
    }

    // 9. Convert UEFI memory descriptors into unified MemoryRegion array
    let mut count = 0usize;
    let mut offset = 0usize;

    while offset + descriptor_size <= map_size && count < 256 {
        let desc = unsafe {
            &*(memory_map_buffer.as_ptr().add(offset) as *const EfiMemoryDescriptor)
        };

        let start = desc.physical_start;
        let end = start + desc.number_of_pages * 4096;

        let kind = match desc.r#type {
            // EfiConventionalMemory
            7 => MemoryRegionKind::Usable,
            // EfiBootServicesCode / Data become usable RAM once Boot Services exit
            3 | 4 => MemoryRegionKind::Usable,
            // EfiACPIReclaimMemory
            9 => MemoryRegionKind::AcpiReclaimable,
            // EfiACPIMemoryNVS
            10 => MemoryRegionKind::AcpiNvs,
            // EfiUnusableMemory
            8 => MemoryRegionKind::BadMemory,
            // MemoryMappedIO / PortSpace
            11 | 12 => MemoryRegionKind::Mmio,
            // EfiLoaderCode
            1 => MemoryRegionKind::Kernel,
            // EfiLoaderData
            2 => MemoryRegionKind::Bootloader,
            // Reserved / Runtime
            _ => MemoryRegionKind::Reserved,
        };

        unsafe {
            let regions_ptr = KERNEL_REGIONS.0.get();
            (*regions_ptr)[count] = MemoryRegion { start, end, kind };
        }
        count += 1;
        offset += descriptor_size;
    }

    // 10. Populate the static BootInfo
    let regions_slice: &'static [MemoryRegion] = unsafe {
        let regions_ptr = KERNEL_REGIONS.0.get();
        core::slice::from_raw_parts((*regions_ptr).as_ptr(), count)
    };

    let boot_info = BootInfo {
        magic: BOOT_INFO_MAGIC,
        memory_map: regions_slice,
        acpi_rsdp,
        boot_method: BootMethod::Uefi,
    };

    let boot_info_ptr = KERNEL_BOOT_INFO.0.get();
    unsafe {
        *boot_info_ptr = Some(boot_info);
    }

    let boot_info_addr = unsafe { (*boot_info_ptr).as_ref().unwrap() as *const BootInfo as u64 };

    // 11. Switch to our 4-level page table, switch stack, and jump to `_start` / `kernel_main`
    unsafe {
        core::arch::asm!(
            "mov cr3, {cr3}",
            "mov rsp, {rsp}",
            "mov rdi, {arg}",
            "jmp {entry}",
            cr3 = in(reg) page_tables_addr,
            rsp = in(reg) stack_top,
            arg = in(reg) boot_info_addr,
            entry = in(reg) entry_point,
            options(noreturn)
        );
    }
}

/// Panic handler for the UEFI bootloader.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}
