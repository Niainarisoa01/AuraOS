//! ============================================================================
//! Advanced Configuration and Power Interface (ACPI) Subsystem
//! ============================================================================
//!
//! Provides bare-metal discovery and parsing of ACPI tables:
//!   - RSDP (Root System Description Pointer) scanning in EBDA and BIOS ROM
//!   - RSDT (32-bit) and XSDT (64-bit) table enumeration
//!   - FADT (Fixed ACPI Description Table) — PM1 control registers for power off
//!   - DSDT AML parsing for `_S5` (Soft Off) sleep state values
//!   - MADT (Multiple APIC Description Table) — Local APIC and CPU core enumeration

#![allow(dead_code)]

use alloc::vec::Vec;
use crate::sync::Spinlock;

/// Standard ACPI System Description Table (SDT) Header (36 bytes).
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct SdtHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

/// RSDP Descriptor v1.0 (20 bytes).
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct RsdpDescriptorV1 {
    pub signature: [u8; 8],     // "RSD PTR "
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,           // 0 for ACPI 1.0, 2 for ACPI 2.0+
    pub rsdt_address: u32,
}

/// RSDP Descriptor v2.0 (36 bytes).
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct RsdpDescriptorV2 {
    pub v1: RsdpDescriptorV1,
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    pub reserved: [u8; 3],
}

/// Information about an enumerated CPU core discovered in MADT.
#[derive(Clone, Copy, Debug)]
pub struct CpuCore {
    pub processor_id: u8,
    pub apic_id: u8,
    pub is_enabled: bool,
}

/// Information about an I/O APIC discovered in MADT.
#[derive(Clone, Copy, Debug)]
pub struct IoApicInfo {
    pub id: u8,
    pub address: u32,
    pub gsi_base: u32,
}

/// Central parsed ACPI state.
pub struct AcpiInfo {
    pub is_initialized: bool,
    pub rsdp_addr: u64,
    pub rsdt_addr: u64,
    pub xsdt_addr: u64,
    pub fadt_addr: u64,
    pub madt_addr: u64,
    pub dsdt_addr: u64,
    pub oem_id: [u8; 6],
    pub smi_cmd: u32,
    pub acpi_enable: u8,
    pub pm1a_cnt_blk: u32,
    pub pm1b_cnt_blk: u32,
    pub slp_typa: u16,
    pub slp_typb: u16,
    pub has_s5: bool,
    pub lapic_addr: u32,
    pub cores: Vec<CpuCore>,
    pub io_apics: Vec<IoApicInfo>,
    pub tables_count: usize,
}

impl AcpiInfo {
    pub const fn new() -> Self {
        AcpiInfo {
            is_initialized: false,
            rsdp_addr: 0,
            rsdt_addr: 0,
            xsdt_addr: 0,
            fadt_addr: 0,
            madt_addr: 0,
            dsdt_addr: 0,
            oem_id: [0; 6],
            smi_cmd: 0,
            acpi_enable: 0,
            pm1a_cnt_blk: 0,
            pm1b_cnt_blk: 0,
            slp_typa: 0,
            slp_typb: 0,
            has_s5: false,
            lapic_addr: 0xFEE00000, // Standard default x86 Local APIC base
            cores: Vec::new(),
            io_apics: Vec::new(),
            tables_count: 0,
        }
    }
}

pub static ACPI_DATA: Spinlock<AcpiInfo> = Spinlock::new(AcpiInfo::new());

/// Calculates an 8-bit checksum over a byte slice. Valid ACPI tables sum to 0 mod 256.
pub fn validate_checksum(data: &[u8]) -> bool {
    let sum = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    sum == 0
}

/// Scans physical memory for the ACPI Root System Description Pointer (RSDP).
///
/// Looks in:
/// 1. Extended BIOS Data Area (EBDA)
/// 2. Main BIOS Read-Only Memory (0x000E0000 .. 0x000FFFFF)
pub fn find_rsdp() -> Option<(u64, RsdpDescriptorV1, Option<RsdpDescriptorV2>)> {
    // 1. Scan Main BIOS Read-Only Memory (0x000E0000 .. 0x000FFFFF, 128 KiB)
    if let Some(res) = scan_region_for_rsdp(0x000E_0000, 0x20000) {
        return Some(res);
    }

    // 2. Scan standard EBDA region (0x0009FC00 .. 0x000A0000, 1 KiB)
    if let Some(res) = scan_region_for_rsdp(0x0009_FC00, 0x400) {
        return Some(res);
    }

    // 3. Scan broader EBDA range (0x00080000 .. 0x0009FC00)
    scan_region_for_rsdp(0x0008_0000, 0x1FC00)
}

fn scan_region_for_rsdp(base: u64, len: u64) -> Option<(u64, RsdpDescriptorV1, Option<RsdpDescriptorV2>)> {
    let mut addr = base;
    let end = base + len;

    while addr < end {
        let sig = unsafe { core::ptr::read_volatile(addr as *const [u8; 8]) };
        if &sig == b"RSD PTR " {
            // Validate v1 checksum (first 20 bytes)
            let v1_bytes = unsafe { core::slice::from_raw_parts(addr as *const u8, 20) };
            if validate_checksum(v1_bytes) {
                let v1 = unsafe { core::ptr::read_unaligned(addr as *const RsdpDescriptorV1) };

                // Check for ACPI 2.0+ extension
                let v2 = if v1.revision >= 2 {
                    let v2_raw = unsafe { core::ptr::read_unaligned(addr as *const RsdpDescriptorV2) };
                    let total_len = (v2_raw.length as usize).min(1024);
                    if total_len >= 36 {
                        let v2_bytes = unsafe { core::slice::from_raw_parts(addr as *const u8, total_len) };
                        if validate_checksum(v2_bytes) {
                            Some(v2_raw)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                return Some((addr, v1, v2));
            }
        }
        addr += 16; // RSDP is always aligned to a 16-byte boundary
    }

    None
}

/// Parses the DSDT AML byte stream to find the `_S5` sleep package.
fn parse_s5_from_dsdt(dsdt_addr: u64) -> Option<(u16, u16)> {
    if dsdt_addr == 0 {
        return None;
    }

    let header = unsafe { core::ptr::read_unaligned(dsdt_addr as *const SdtHeader) };
    let len = header.length as usize;
    if len < 36 || len > 0x100000 {
        return None;
    }

    let aml = unsafe { core::slice::from_raw_parts(dsdt_addr as *const u8, len) };

    // Search for the AML name sequence: `_S5_`
    let s5_sig = b"_S5_";
    for i in 0..(aml.len().saturating_sub(16)) {
        if &aml[i..i + 4] == s5_sig {
            // Check if preceded by NameOp (0x08) or followed by PackageOp (0x12)
            let mut ptr = i + 4;
            // Skip NameOp if present
            if i > 0 && aml[i - 1] == 0x08 {
                // Good match
            }

            // Expect PackageOp 0x12
            if ptr < aml.len() && aml[ptr] == 0x12 {
                ptr += 1;
                // Skip PkgLength (1 to 4 bytes)
                let pkg_lead = aml[ptr];
                let byte_count = (pkg_lead >> 6) as usize;
                ptr += 1 + byte_count;

                // Number of elements
                if ptr < aml.len() {
                    let _num_elements = aml[ptr];
                    ptr += 1;

                    // Extract SLP_TYPa
                    let slp_a = parse_aml_byte_integer(aml, &mut ptr)?;
                    // Extract SLP_TYPb
                    let slp_b = parse_aml_byte_integer(aml, &mut ptr).unwrap_or(slp_a);

                    return Some((slp_a as u16, slp_b as u16));
                }
            }
        }
    }

    None
}

/// Helper to decode a single AML integer byte.
fn parse_aml_byte_integer(aml: &[u8], ptr: &mut usize) -> Option<u8> {
    if *ptr >= aml.len() {
        return None;
    }
    let opcode = aml[*ptr];
    *ptr += 1;

    match opcode {
        0x00 => Some(0), // ZeroOp
        0x01 => Some(1), // OneOp
        0xFF => Some(0xFF), // OnesOp
        0x0A => {
            // BytePrefix
            if *ptr < aml.len() {
                let val = aml[*ptr];
                *ptr += 1;
                Some(val)
            } else {
                None
            }
        }
        val if val <= 0x3F => Some(val),
        _ => None,
    }
}

/// Parses the Multiple APIC Description Table (MADT).
fn parse_madt(madt_addr: u64, info: &mut AcpiInfo) {
    if madt_addr == 0 {
        return;
    }

    let header = unsafe { core::ptr::read_unaligned(madt_addr as *const SdtHeader) };
    let total_len = header.length as usize;
    if total_len < 44 || total_len > 0x10000 {
        return;
    }

    // Offset 36: Local Interrupt Controller (LAPIC) Base Address (32 bits)
    let lapic_base = unsafe { core::ptr::read_unaligned((madt_addr + 36) as *const u32) };
    info.lapic_addr = lapic_base;

    let data = unsafe { core::slice::from_raw_parts(madt_addr as *const u8, total_len) };
    let mut offset = 44; // Records start after header (36B) + LAPIC (4B) + Flags (4B)

    while offset + 2 <= total_len {
        let entry_type = data[offset];
        let entry_len = data[offset + 1] as usize;

        if entry_len < 2 || offset + entry_len > total_len {
            break;
        }

        match entry_type {
            0 => {
                // Type 0: Processor Local APIC
                if entry_len >= 8 {
                    let processor_id = data[offset + 2];
                    let apic_id = data[offset + 3];
                    let flags = u32::from_le_bytes([
                        data[offset + 4],
                        data[offset + 5],
                        data[offset + 6],
                        data[offset + 7],
                    ]);

                    let is_enabled = (flags & 1) != 0 || (flags & 2) != 0;
                    info.cores.push(CpuCore {
                        processor_id,
                        apic_id,
                        is_enabled,
                    });
                }
            }
            1 => {
                // Type 1: I/O APIC
                if entry_len >= 12 {
                    let id = data[offset + 2];
                    let address = u32::from_le_bytes([
                        data[offset + 4],
                        data[offset + 5],
                        data[offset + 6],
                        data[offset + 7],
                    ]);
                    let gsi_base = u32::from_le_bytes([
                        data[offset + 8],
                        data[offset + 9],
                        data[offset + 10],
                        data[offset + 11],
                    ]);

                    info.io_apics.push(IoApicInfo {
                        id,
                        address,
                        gsi_base,
                    });
                }
            }
            _ => {}
        }

        offset += entry_len;
    }
}

/// Parses the Fixed ACPI Description Table (FADT).
fn parse_fadt(fadt_addr: u64, info: &mut AcpiInfo) {
    if fadt_addr == 0 {
        return;
    }

    let header = unsafe { core::ptr::read_unaligned(fadt_addr as *const SdtHeader) };
    let total_len = header.length as usize;
    if total_len < 76 {
        return;
    }

    let data = unsafe { core::slice::from_raw_parts(fadt_addr as *const u8, total_len) };

    // Offset 40: DSDT 32-bit pointer
    let dsdt_32 = u32::from_le_bytes([data[40], data[41], data[42], data[43]]) as u64;

    // Offset 48: SMI_CMD I/O Port
    let smi_cmd = u32::from_le_bytes([data[48], data[49], data[50], data[51]]);
    let acpi_enable = data[52];

    // Offset 64: PM1a_CNT_BLK I/O Port
    let pm1a_cnt = u32::from_le_bytes([data[64], data[65], data[66], data[67]]);
    // Offset 68: PM1b_CNT_BLK I/O Port
    let pm1b_cnt = u32::from_le_bytes([data[68], data[69], data[70], data[71]]);

    info.dsdt_addr = dsdt_32;
    info.smi_cmd = smi_cmd;
    info.acpi_enable = acpi_enable;
    info.pm1a_cnt_blk = pm1a_cnt;
    info.pm1b_cnt_blk = pm1b_cnt;

    // In ACPI 2.0+, 64-bit X_DSDT is at offset 140
    if total_len >= 148 {
        let mut x_dsdt_bytes = [0u8; 8];
        x_dsdt_bytes.copy_from_slice(&data[140..148]);
        let x_dsdt = u64::from_le_bytes(x_dsdt_bytes);
        if x_dsdt != 0 {
            info.dsdt_addr = x_dsdt;
        }
    }

    // Try to parse `_S5` from DSDT
    if let Some((slp_a, slp_b)) = parse_s5_from_dsdt(info.dsdt_addr) {
        info.slp_typa = slp_a;
        info.slp_typb = slp_b;
        info.has_s5 = true;
    } else {
        // Standard default QEMU / Bochs S5 sleep type values
        info.slp_typa = 0x00;
        info.slp_typb = 0x00;
        info.has_s5 = false;
    }
}

/// Validates and parses an RSDP descriptor at a specific physical address.
pub fn parse_rsdp_at(addr: u64) -> Option<(u64, RsdpDescriptorV1, Option<RsdpDescriptorV2>)> {
    if addr == 0 {
        return None;
    }
    let sig = unsafe { core::ptr::read_volatile(addr as *const [u8; 8]) };
    if &sig != b"RSD PTR " {
        return None;
    }
    let v1_bytes = unsafe { core::slice::from_raw_parts(addr as *const u8, 20) };
    if !validate_checksum(v1_bytes) {
        return None;
    }
    let v1 = unsafe { core::ptr::read_unaligned(addr as *const RsdpDescriptorV1) };
    let v2 = if v1.revision >= 2 {
        let v2_raw = unsafe { core::ptr::read_unaligned(addr as *const RsdpDescriptorV2) };
        let total_len = (v2_raw.length as usize).min(1024);
        if total_len >= 36 {
            let v2_bytes = unsafe { core::slice::from_raw_parts(addr as *const u8, total_len) };
            if validate_checksum(v2_bytes) {
                Some(v2_raw)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    Some((addr, v1, v2))
}

/// Initializes the ACPI subsystem with an optional RSDP pointer override.
///
/// If `rsdp_override` is `Some(addr)`, the function validates and parses the RSDP
/// directly at that physical address (e.g. obtained via EFI Configuration Table).
/// If `None` or invalid, it falls back to scanning physical memory (EBDA / BIOS ROM).
pub fn init_with_rsdp(rsdp_override: Option<u64>) -> bool {
    let mut info = ACPI_DATA.lock();

    let (rsdp_addr, rsdp_v1, rsdp_v2) = if let Some(addr) = rsdp_override {
        if let Some(res) = parse_rsdp_at(addr) {
            res
        } else if let Some(res) = find_rsdp() {
            res
        } else {
            crate::serial_println!("[ACPI] Warning: Provided RSDP at {:#x} invalid and scan failed.", addr);
            return false;
        }
    } else if let Some(res) = find_rsdp() {
        res
    } else {
        crate::serial_println!("[ACPI] Warning: RSDP structure not found in EBDA or BIOS ROM.");
        return false;
    };

    info.rsdp_addr = rsdp_addr;
    info.oem_id = rsdp_v1.oem_id;

    // Check whether XSDT (64-bit) or RSDT (32-bit) should be used
    let mut table_addrs = Vec::new();

    if let Some(v2) = rsdp_v2 {
        if v2.xsdt_address != 0 {
            info.xsdt_addr = v2.xsdt_address;
            let header = unsafe { core::ptr::read_unaligned(v2.xsdt_address as *const SdtHeader) };
            let total_len = header.length as usize;
            if total_len >= 36 && total_len <= 0x10000 {
                let entry_count = (total_len - 36) / 8;
                let ptr_base = (v2.xsdt_address + 36) as *const u64;
                for i in 0..entry_count {
                    let addr = unsafe { core::ptr::read_unaligned(ptr_base.add(i)) };
                    if addr != 0 {
                        table_addrs.push(addr);
                    }
                }
            }
        }
    }

    // Fall back to RSDT (32-bit pointers)
    if table_addrs.is_empty() && rsdp_v1.rsdt_address != 0 {
        info.rsdt_addr = rsdp_v1.rsdt_address as u64;
        let header = unsafe { core::ptr::read_unaligned(info.rsdt_addr as *const SdtHeader) };
        let total_len = header.length as usize;
        if total_len >= 36 && total_len <= 0x10000 {
            let entry_count = (total_len - 36) / 4;
            let ptr_base = (info.rsdt_addr + 36) as *const u32;
            for i in 0..entry_count {
                let addr = unsafe { core::ptr::read_unaligned(ptr_base.add(i)) } as u64;
                if addr != 0 {
                    table_addrs.push(addr);
                }
            }
        }
    }

    info.tables_count = table_addrs.len();

    // Inspect each referenced ACPI table
    for &addr in &table_addrs {
        let header = unsafe { core::ptr::read_unaligned(addr as *const SdtHeader) };
        match &header.signature {
            b"FACP" => {
                info.fadt_addr = addr;
            }
            b"APIC" => {
                info.madt_addr = addr;
            }
            _ => {}
        }
    }

    // Parse FADT and MADT
    let fadt = info.fadt_addr;
    let madt = info.madt_addr;

    if fadt != 0 {
        parse_fadt(fadt, &mut info);
    }
    if madt != 0 {
        parse_madt(madt, &mut info);
    }

    // If no MADT cores were enumerated (e.g. minimal emulation), register BSP as core 0
    if info.cores.is_empty() {
        info.cores.push(CpuCore {
            processor_id: 0,
            apic_id: 0,
            is_enabled: true,
        });
    }

    info.is_initialized = true;

    crate::serial_println!(
        "[ACPI] Initialized. Tables: {}, OEM: {:?}, FADT: {:#x}, MADT: {:#x} ({} CPU core(s), LAPIC: {:#x})",
        info.tables_count,
        core::str::from_utf8(&info.oem_id).unwrap_or("???"),
        info.fadt_addr,
        info.madt_addr,
        info.cores.len(),
        info.lapic_addr
    );

    true
}

/// Initializes the ACPI subsystem using automatic physical memory scanning (BIOS fallback).
pub fn init() -> bool {
    init_with_rsdp(None)
}
