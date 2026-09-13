//! ============================================================================
//! AuraOS UEFI Subsystem — Pure no_std Handcrafted UEFI ABI
//! ============================================================================
//!
//! Provides zero-dependency definitions for the standard UEFI 2.x ABI:
//! - Data types, handles, status codes, and GUID representations.
//! - System Table, Boot Services, Configuration Table.
//! - Console Text Output Protocol.
//! - Memory Map descriptors and Page Allocation services.

#![allow(dead_code)]

pub type EfiHandle = *mut core::ffi::c_void;
pub type EfiStatus = usize;

pub const EFI_SUCCESS: EfiStatus = 0;
pub const EFI_LOAD_ERROR: EfiStatus = 1 | (1usize << (usize::BITS - 1));
pub const EFI_INVALID_PARAMETER: EfiStatus = 2 | (1usize << (usize::BITS - 1));
pub const EFI_UNSUPPORTED: EfiStatus = 3 | (1usize << (usize::BITS - 1));
pub const EFI_BAD_BUFFER_SIZE: EfiStatus = 4 | (1usize << (usize::BITS - 1));
pub const EFI_BUFFER_TOO_SMALL: EfiStatus = 5 | (1usize << (usize::BITS - 1));
pub const EFI_NOT_READY: EfiStatus = 6 | (1usize << (usize::BITS - 1));
pub const EFI_NOT_FOUND: EfiStatus = 14 | (1usize << (usize::BITS - 1));

/// 128-bit globally unique identifier (GUID).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EfiGuid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl EfiGuid {
    pub const fn new(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Self {
        EfiGuid { data1, data2, data3, data4 }
    }
}

/// ACPI 2.0+ Root System Description Pointer (RSDP) table GUID in UEFI Configuration Table.
/// {8868e871-e4f1-11d3-bc22-0080c73c8881}
pub const EFI_ACPI_20_TABLE_GUID: EfiGuid = EfiGuid::new(
    0x8868e871, 0xe4f1, 0x11d3,
    [0xbc, 0x22, 0x00, 0x80, 0xc7, 0x3c, 0x88, 0x81]
);

/// ACPI 1.0 Root System Description Pointer (RSDP) table GUID in UEFI Configuration Table.
/// {eb9d2d30-2d88-11d3-9a16-0090273fc14d}
pub const ACPI_10_TABLE_GUID: EfiGuid = EfiGuid::new(
    0xeb9d2d30, 0x2d88, 0x11d3,
    [0x9a, 0x16, 0x00, 0x90, 0x27, 0x3f, 0xc1, 0x4d]
);

/// Loaded Image Protocol GUID.
/// {5B1B31A1-9562-11d2-8E3F-00A0C969723B}
pub const EFI_LOADED_IMAGE_PROTOCOL_GUID: EfiGuid = EfiGuid::new(
    0x5b1b31a1, 0x9562, 0x11d2,
    [0x8e, 0x3f, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b]
);

/// Simple File System Protocol GUID.
/// {0964e5b22-6459-11d2-8e39-00a0c969723b}
pub const EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID: EfiGuid = EfiGuid::new(
    0x964e5b22, 0x6459, 0x11d2,
    [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b]
);

/// Standard table header present at the start of all UEFI tables.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EfiTableHeader {
    pub signature: u64,
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}

/// Simple Text Output Protocol for console display.
#[repr(C)]
pub struct EfiSimpleTextOutputProtocol {
    pub reset: unsafe extern "efiapi" fn(
        this: *mut EfiSimpleTextOutputProtocol,
        extended_verification: bool,
    ) -> EfiStatus,
    pub output_string: unsafe extern "efiapi" fn(
        this: *mut EfiSimpleTextOutputProtocol,
        string: *const u16,
    ) -> EfiStatus,
    pub test_string: usize,
    pub query_mode: usize,
    pub set_mode: usize,
    pub set_attribute: unsafe extern "efiapi" fn(
        this: *mut EfiSimpleTextOutputProtocol,
        attribute: usize,
    ) -> EfiStatus,
    pub clear_screen: unsafe extern "efiapi" fn(
        this: *mut EfiSimpleTextOutputProtocol,
    ) -> EfiStatus,
    pub set_cursor_position: usize,
    pub enable_cursor: usize,
    pub mode: *mut core::ffi::c_void,
}

/// Entry in the UEFI System Configuration Table.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EfiConfigurationTable {
    pub vendor_guid: EfiGuid,
    pub vendor_table: *const core::ffi::c_void,
}

/// Classification of physical memory descriptors reported by UEFI.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EfiMemoryType {
    ReservedMemoryType = 0,
    LoaderCode = 1,
    LoaderData = 2,
    BootServicesCode = 3,
    BootServicesData = 4,
    RuntimeServicesCode = 5,
    RuntimeServicesData = 6,
    ConventionalMemory = 7,
    UnusableMemory = 8,
    ACPIReclaimMemory = 9,
    ACPIMemoryNVS = 10,
    MemoryMappedIO = 11,
    MemoryMappedIOPortSpace = 12,
    PalCode = 13,
    PersistentMemory = 14,
}

/// Strategy for page allocations in UEFI.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EfiAllocateType {
    AllocateAnyPages = 0,
    AllocateMaxAddress = 1,
    AllocateAddress = 2,
}

/// A physical memory descriptor entry returned by `GetMemoryMap`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EfiMemoryDescriptor {
    pub r#type: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

/// UEFI Boot Services Table.
#[repr(C)]
pub struct EfiBootServices {
    pub hdr: EfiTableHeader,

    // Task Priority Services
    pub raise_tpl: usize,
    pub restore_tpl: usize,

    // Memory Services
    pub allocate_pages: unsafe extern "efiapi" fn(
        alloc_type: EfiAllocateType,
        memory_type: EfiMemoryType,
        pages: usize,
        memory: *mut u64,
    ) -> EfiStatus,
    pub free_pages: unsafe extern "efiapi" fn(
        memory: u64,
        pages: usize,
    ) -> EfiStatus,
    pub get_memory_map: unsafe extern "efiapi" fn(
        memory_map_size: *mut usize,
        memory_map: *mut EfiMemoryDescriptor,
        map_key: *mut usize,
        descriptor_size: *mut usize,
        descriptor_version: *mut u32,
    ) -> EfiStatus,
    pub allocate_pool: unsafe extern "efiapi" fn(
        pool_type: EfiMemoryType,
        size: usize,
        buffer: *mut *mut core::ffi::c_void,
    ) -> EfiStatus,
    pub free_pool: unsafe extern "efiapi" fn(
        buffer: *mut core::ffi::c_void,
    ) -> EfiStatus,

    // Event & Timer Services
    pub create_event: usize,
    pub set_timer: usize,
    pub wait_for_event: usize,
    pub signal_event: usize,
    pub close_event: usize,
    pub check_event: usize,

    // Protocol Handler Services
    pub install_protocol_interface: usize,
    pub reinstall_protocol_interface: usize,
    pub uninstall_protocol_interface: usize,
    pub handle_protocol: unsafe extern "efiapi" fn(
        handle: EfiHandle,
        protocol: *const EfiGuid,
        interface: *mut *mut core::ffi::c_void,
    ) -> EfiStatus,
    pub pcr_reserved: usize,
    pub register_protocol_notify: usize,
    pub locate_handle: usize,
    pub locate_device_path: usize,
    pub install_configuration_table: usize,

    // Image Services
    pub load_image: usize,
    pub start_image: usize,
    pub exit: usize,
    pub unload_image: usize,
    pub exit_boot_services: unsafe extern "efiapi" fn(
        image_handle: EfiHandle,
        map_key: usize,
    ) -> EfiStatus,

    // Miscellaneous Services
    pub get_next_monotonic_count: usize,
    pub stall: unsafe extern "efiapi" fn(microseconds: usize) -> EfiStatus,
    pub set_watchdog_timer: unsafe extern "efiapi" fn(
        timeout: usize,
        watchdog_code: u64,
        data_size: usize,
        watchdog_data: *const u16,
    ) -> EfiStatus,
}

/// Primary UEFI System Table passed to `efi_main`.
#[repr(C)]
pub struct EfiSystemTable {
    pub hdr: EfiTableHeader,
    pub firmware_vendor: *const u16,
    pub firmware_revision: u32,
    pub console_in_handle: EfiHandle,
    pub con_in: *mut core::ffi::c_void,
    pub console_out_handle: EfiHandle,
    pub con_out: *mut EfiSimpleTextOutputProtocol,
    pub standard_error_handle: EfiHandle,
    pub std_err: *mut EfiSimpleTextOutputProtocol,
    pub runtime_services: *mut core::ffi::c_void,
    pub boot_services: *mut EfiBootServices,
    pub number_of_table_entries: usize,
    pub configuration_table: *const EfiConfigurationTable,
}
