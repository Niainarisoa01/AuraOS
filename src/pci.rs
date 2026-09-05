/// ============================================================================
/// PCI Bus Driver & Hardware Enumeration
/// ============================================================================
///
/// The Peripheral Component Interconnect (PCI) bus allows the operating system
/// to discover and configure hardware devices (VGA graphics, network cards,
/// SATA/NVMe controllers, USB hosts, VirtIO devices, etc.).
///
/// Configuration access is performed via x86 I/O ports:
/// - 0xCF8: PCI Configuration Address (CONFIG_ADDRESS)
/// - 0xCFC: PCI Configuration Data (CONFIG_DATA)

use crate::io::{inl, outl};
use alloc::vec::Vec;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// Represents a detected PCI device on the bus.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_id: u8,
    pub subclass_id: u8,
    pub prog_if: u8,
    pub revision_id: u8,
    pub header_type: u8,
}

impl PciDevice {
    /// Returns a human-readable vendor name if recognized.
    pub fn vendor_name(&self) -> &'static str {
        match self.vendor_id {
            0x8086 => "Intel Corp.",
            0x1022 => "AMD Inc.",
            0x10DE => "NVIDIA Corp.",
            0x10EC => "Realtek Semiconductor",
            0x1AF4 => "Red Hat / VirtIO",
            0x1234 => "Bochs / QEMU Emulator",
            0x15AD => "VMware Inc.",
            0x80EE => "VirtualBox (InnoTek)",
            0x1013 => "Cirrus Logic",
            _ => "Unknown Vendor",
        }
    }

    /// Returns a human-readable description of the device class.
    pub fn class_name(&self) -> &'static str {
        match self.class_id {
            0x00 => "Legacy Device",
            0x01 => match self.subclass_id {
                0x00 => "SCSI Storage Controller",
                0x01 => "IDE Storage Controller",
                0x06 => "SATA / AHCI Controller",
                0x08 => "NVMe Storage Controller",
                _ => "Mass Storage Controller",
            },
            0x02 => match self.subclass_id {
                0x00 => "Ethernet Network Controller",
                0x80 => "Other Network Controller",
                _ => "Network Controller",
            },
            0x03 => match self.subclass_id {
                0x00 => "VGA Compatible Controller",
                0x02 => "3D Graphics Controller",
                _ => "Display Controller",
            },
            0x04 => match self.subclass_id {
                0x03 => "High Definition Audio Device",
                _ => "Multimedia Controller",
            },
            0x05 => "Memory Controller",
            0x06 => match self.subclass_id {
                0x00 => "Host / PCI Bridge",
                0x01 => "ISA Bridge",
                0x04 => "PCI-to-PCI Bridge",
                _ => "Bridge Device",
            },
            0x07 => "Communication Controller",
            0x08 => "Generic System Peripheral",
            0x0C => match self.subclass_id {
                0x03 => "USB Host Controller",
                0x05 => "SMBus Controller",
                _ => "Serial Bus Controller",
            },
            _ => "Other Peripheral Device",
        }
    }
}

/// Reads a 32-bit register from the PCI configuration space.
pub fn pci_read_config_u32(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address = ((1u32) << 31)
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        inl(PCI_CONFIG_DATA)
    }
}

/// Reads a 16-bit register from the PCI configuration space.
pub fn pci_read_config_u16(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    let reg32 = pci_read_config_u32(bus, slot, func, offset);
    let shift = (offset & 2) * 8;
    ((reg32 >> shift) & 0xFFFF) as u16
}

/// Reads an 8-bit register from the PCI configuration space.
pub fn pci_read_config_u8(bus: u8, slot: u8, func: u8, offset: u8) -> u8 {
    let reg32 = pci_read_config_u32(bus, slot, func, offset);
    let shift = (offset & 3) * 8;
    ((reg32 >> shift) & 0xFF) as u8
}

/// Scans the PCI bus and returns all discovered active devices.
pub fn scan_pci_bus() -> Vec<PciDevice> {
    let mut devices = Vec::new();

    // Scan the first 8 buses (common for PCs and VMs)
    for bus in 0..8 {
        for slot in 0..32 {
            // Check function 0 first
            let vendor_id = pci_read_config_u16(bus, slot, 0, 0x00);
            if vendor_id == 0xFFFF {
                // Device does not exist
                continue;
            }

            let header_type = pci_read_config_u8(bus, slot, 0, 0x0E);
            let is_multi_function = (header_type & 0x80) != 0;
            let max_funcs = if is_multi_function { 8 } else { 1 };

            for func in 0..max_funcs {
                let func_vendor_id = pci_read_config_u16(bus, slot, func, 0x00);
                if func_vendor_id == 0xFFFF {
                    continue;
                }

                let device_id = pci_read_config_u16(bus, slot, func, 0x02);
                let class_id = pci_read_config_u8(bus, slot, func, 0x0B);
                let subclass_id = pci_read_config_u8(bus, slot, func, 0x0A);
                let prog_if = pci_read_config_u8(bus, slot, func, 0x09);
                let revision_id = pci_read_config_u8(bus, slot, func, 0x08);
                let func_header_type = pci_read_config_u8(bus, slot, func, 0x0E);

                devices.push(PciDevice {
                    bus,
                    slot,
                    func,
                    vendor_id: func_vendor_id,
                    device_id,
                    class_id,
                    subclass_id,
                    prog_if,
                    revision_id,
                    header_type: func_header_type,
                });
            }
        }
    }

    devices
}
