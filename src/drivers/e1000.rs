//! ============================================================================
//! Intel 82540EM (e1000) Gigabit Ethernet Controller Driver
//! ============================================================================
//!
//! Implements a pure `#![no_std]` Rust driver for the Intel 82540EM (e1000)
//! network interface card (NIC), the standard Gigabit Ethernet device in QEMU.
//!
//! Features:
//!   - PCI discovery (`8086:100E`), Bus Master activation
//!   - Dual MMIO and I/O Port register access (`IOADDR` / `IODATA`)
//!   - Reading hardware MAC address from RAL0/RAH0 and EEPROM
//!   - Circular DMA ring buffers for Transmit (TX) and Receive (RX) descriptors
//!   - Non-blocking packet reception and transmission

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use crate::arch::io::{inl, outl};
use crate::memory::paging::virt_to_phys;
use crate::sync::Spinlock;

// Intel e1000 PCI Vendor and Device IDs
pub const E1000_VENDOR_ID: u16 = 0x8086;
pub const E1000_DEVICE_82540EM: u16 = 0x100E;
pub const E1000_DEVICE_82545EM: u16 = 0x100F;
pub const E1000_DEVICE_82543GC: u16 = 0x1004;
pub const E1000_DEVICE_I217: u16 = 0x1539;

// Intel e1000 Register Offsets
pub const REG_CTRL: u32 = 0x0000;    // Device Control
pub const REG_STATUS: u32 = 0x0008;  // Device Status
pub const REG_EECD: u32 = 0x0010;    // EEPROM Control
pub const REG_EERD: u32 = 0x0014;    // EEPROM Read
pub const REG_ICR: u32 = 0x00C0;     // Interrupt Cause Read
pub const REG_IMS: u32 = 0x00D0;     // Interrupt Mask Set
pub const REG_IMC: u32 = 0x00D8;     // Interrupt Mask Clear
pub const REG_RCTL: u32 = 0x0100;    // Receive Control
pub const REG_TCTL: u32 = 0x0400;    // Transmit Control
pub const REG_TIPG: u32 = 0x0410;    // Transmit Inter-Packet Gap
pub const REG_RDBAL: u32 = 0x2800;   // RX Descriptor Base Low
pub const REG_RDBAH: u32 = 0x2804;   // RX Descriptor Base High
pub const REG_RDLEN: u32 = 0x2808;   // RX Descriptor Length
pub const REG_RDH: u32 = 0x2810;     // RX Descriptor Head
pub const REG_RDT: u32 = 0x2818;     // RX Descriptor Tail
pub const REG_TDBAL: u32 = 0x3800;   // TX Descriptor Base Low
pub const REG_TDBAH: u32 = 0x3804;   // TX Descriptor Base High
pub const REG_TDLEN: u32 = 0x3808;   // TX Descriptor Length
pub const REG_TDH: u32 = 0x3810;     // TX Descriptor Head
pub const REG_TDT: u32 = 0x3818;     // TX Descriptor Tail
pub const REG_RAL0: u32 = 0x5400;    // Receive Address Low 0 (MAC 0..3)
pub const REG_RAH0: u32 = 0x5404;    // Receive Address High 0 (MAC 4..5)

// Control Register (CTRL) Bits
pub const CTRL_FD: u32 = 1 << 0;     // Full Duplex
pub const CTRL_LRST: u32 = 1 << 3;   // Link Reset
pub const CTRL_ASDE: u32 = 1 << 5;   // Auto-Speed Detection Enable
pub const CTRL_SLU: u32 = 1 << 6;    // Set Link Up
pub const CTRL_RST: u32 = 1 << 26;   // Device Reset

// Receive Control (RCTL) Bits
pub const RCTL_EN: u32 = 1 << 1;     // Receiver Enable
pub const RCTL_SBP: u32 = 1 << 2;    // Store Bad Packets
pub const RCTL_UPE: u32 = 1 << 3;    // Unicast Promiscuous Enable
pub const RCTL_MPE: u32 = 1 << 4;    // Multicast Promiscuous Enable
pub const RCTL_LPE: u32 = 1 << 5;    // Long Packet Enable
pub const RCTL_BAM: u32 = 1 << 15;   // Broadcast Accept Mode
pub const RCTL_BSIZE_2048: u32 = 0;  // 2048-byte buffer
pub const RCTL_SECRC: u32 = 1 << 26; // Strip Ethernet CRC

// Transmit Control (TCTL) Bits
pub const TCTL_EN: u32 = 1 << 1;     // Transmit Enable
pub const TCTL_PSP: u32 = 1 << 3;    // Pad Short Packets
pub const TCTL_CT: u32 = 0x10 << 4;  // Collision Threshold
pub const TCTL_COLD: u32 = 0x40 << 12; // Collision Distance (Full Duplex)

// Descriptor Counts & Buffer Sizes
pub const RX_DESC_COUNT: usize = 8;
pub const TX_DESC_COUNT: usize = 8;
pub const PACKET_BUFFER_SIZE: usize = 2048;

/// Receive Descriptor (16 bytes, hardware-aligned).
#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
pub struct RxDesc {
    pub buffer_addr: u64, // Physical address of packet buffer
    pub length: u16,
    pub checksum: u16,
    pub status: u8,       // Bit 0: DD (Descriptor Done), Bit 1: EOP (End of Packet)
    pub errors: u8,
    pub special: u16,
}

/// Transmit Descriptor (16 bytes, hardware-aligned).
#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
pub struct TxDesc {
    pub buffer_addr: u64, // Physical address of packet buffer
    pub length: u16,
    pub cso: u8,
    pub cmd: u8,          // Bit 0: EOP, Bit 1: IFCS, Bit 3: RS (Report Status)
    pub status: u8,       // Bit 0: DD (Descriptor Done)
    pub css: u8,
    pub special: u16,
}

/// e1000 Hardware Controller Instance.
pub struct E1000 {
    pub pci_bus: u8,
    pub pci_slot: u8,
    pub pci_func: u8,
    pub mmio_base: u64,
    pub io_base: u16,
    pub mac: [u8; 6],
    pub is_initialized: bool,

    // DMA Rings & Buffers
    rx_descs: Option<Box<[RxDesc]>>,
    rx_buffers: Vec<Box<[u8; PACKET_BUFFER_SIZE]>>,
    rx_cur: usize,

    tx_descs: Option<Box<[TxDesc]>>,
    tx_buffers: Vec<Box<[u8; PACKET_BUFFER_SIZE]>>,
    tx_cur: usize,

    pub rx_packets_count: u64,
    pub tx_packets_count: u64,
    pub rx_bytes_count: u64,
    pub tx_bytes_count: u64,
}

impl E1000 {
    pub const fn new() -> Self {
        E1000 {
            pci_bus: 0,
            pci_slot: 0,
            pci_func: 0,
            mmio_base: 0,
            io_base: 0,
            mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56], // Default QEMU MAC fallback
            is_initialized: false,
            rx_descs: None,
            rx_buffers: Vec::new(),
            rx_cur: 0,
            tx_descs: None,
            tx_buffers: Vec::new(),
            tx_cur: 0,
            rx_packets_count: 0,
            tx_packets_count: 0,
            rx_bytes_count: 0,
            tx_bytes_count: 0,
        }
    }

    /// Reads a 32-bit register from the controller.
    pub fn read_reg(&self, reg: u32) -> u32 {
        if self.io_base != 0 {
            unsafe {
                outl(self.io_base, reg);
                inl(self.io_base + 4)
            }
        } else if self.mmio_base != 0 {
            unsafe {
                core::ptr::read_volatile((self.mmio_base + reg as u64) as *const u32)
            }
        } else {
            0
        }
    }

    /// Writes a 32-bit register on the controller.
    pub fn write_reg(&self, reg: u32, value: u32) {
        if self.io_base != 0 {
            unsafe {
                outl(self.io_base, reg);
                outl(self.io_base + 4, value);
            }
        } else if self.mmio_base != 0 {
            unsafe {
                core::ptr::write_volatile((self.mmio_base + reg as u64) as *mut u32, value);
            }
        }
    }

    /// Reads the hardware MAC address and ensures RAL0/RAH0 are properly programmed.
    fn read_mac_address(&mut self) {
        let ral = self.read_reg(REG_RAL0);
        let rah = self.read_reg(REG_RAH0);

        if ral != 0 || (rah & 0xFFFF) != 0 {
            self.mac[0] = (ral & 0xFF) as u8;
            self.mac[1] = ((ral >> 8) & 0xFF) as u8;
            self.mac[2] = ((ral >> 16) & 0xFF) as u8;
            self.mac[3] = ((ral >> 24) & 0xFF) as u8;
            self.mac[4] = (rah & 0xFF) as u8;
            self.mac[5] = ((rah >> 8) & 0xFF) as u8;
        } else {
            // Fallback to EEPROM reading via EERD
            let mut eeprom_mac = [0u8; 6];
            let mut success = true;

            for word in 0..3 {
                // Try standard 82540 EERD: START bit 0, ADDR shift 2
                self.write_reg(REG_EERD, 1 | ((word as u32) << 2));
                let mut attempts = 0;
                let mut res = self.read_reg(REG_EERD);
                // DONE bit: bit 1 (0x02) in 82540/QEMU, bit 4 (0x10) in older controllers
                while (res & 0x12) == 0 && attempts < 2000 {
                    core::hint::spin_loop();
                    attempts += 1;
                    res = self.read_reg(REG_EERD);
                }

                // If not done with shift 2, retry with shift 8 (legacy/variant layout)
                if (res & 0x12) == 0 {
                    self.write_reg(REG_EERD, 1 | ((word as u32) << 8));
                    attempts = 0;
                    res = self.read_reg(REG_EERD);
                    while (res & 0x12) == 0 && attempts < 2000 {
                        core::hint::spin_loop();
                        attempts += 1;
                        res = self.read_reg(REG_EERD);
                    }
                }

                if (res & 0x12) != 0 {
                    let val = (res >> 16) as u16;
                    eeprom_mac[word * 2] = (val & 0xFF) as u8;
                    eeprom_mac[word * 2 + 1] = ((val >> 8) & 0xFF) as u8;
                } else {
                    success = false;
                }
            }

            if success && eeprom_mac.iter().any(|&b| b != 0) && eeprom_mac != [0xFF; 6] {
                self.mac = eeprom_mac;
            }
        }

        // Ensure MAC address is never all zeroes or broadcast
        if !self.mac.iter().any(|&b| b != 0) || self.mac == [0xFF; 6] {
            self.mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]; // Standard QEMU virtual NIC MAC
        }

        // Program RAL0 and RAH0 with Address Valid (AV, bit 31) so controller filters packets for this MAC
        let ral_val = (self.mac[0] as u32)
            | ((self.mac[1] as u32) << 8)
            | ((self.mac[2] as u32) << 16)
            | ((self.mac[3] as u32) << 24);
        let rah_val = (self.mac[4] as u32)
            | ((self.mac[5] as u32) << 8)
            | (1 << 31); // AV (Address Valid)
        self.write_reg(REG_RAL0, ral_val);
        self.write_reg(REG_RAH0, rah_val);
    }

    /// Initializes the Intel e1000 controller.
    pub fn init(&mut self) -> bool {
        // Step 1: Scan PCI bus to locate e1000 controller
        let pci_devices = crate::drivers::pci::scan_pci_bus();
        let mut found = None;

        for dev in &pci_devices {
            if dev.vendor_id == E1000_VENDOR_ID &&
               (dev.device_id == E1000_DEVICE_82540EM ||
                dev.device_id == E1000_DEVICE_82545EM ||
                dev.device_id == E1000_DEVICE_82543GC ||
                dev.device_id == E1000_DEVICE_I217) {
                found = Some(*dev);
                break;
            }
        }

        let dev = match found {
            Some(d) => d,
            None => {
                crate::serial_println!("[e1000] Controller not found on PCI bus.");
                return false;
            }
        };

        self.pci_bus = dev.bus;
        self.pci_slot = dev.slot;
        self.pci_func = dev.func;

        // Step 2: Enable Bus Master & I/O / Memory space
        crate::drivers::pci::pci_enable_bus_master(dev.bus, dev.slot, dev.func);

        // Step 3: Query BARs
        let (bar0_val, bar0_is_io) = crate::drivers::pci::pci_get_bar(dev.bus, dev.slot, dev.func, 0);
        let (bar1_val, bar1_is_io) = crate::drivers::pci::pci_get_bar(dev.bus, dev.slot, dev.func, 1);

        if bar1_is_io && bar1_val != 0 {
            self.io_base = bar1_val as u16;
        } else if bar0_is_io && bar0_val != 0 {
            self.io_base = bar0_val as u16;
        } else {
            self.mmio_base = bar0_val;
        }

        crate::serial_println!(
            "[e1000] Detected device {:04x}:{:04x} at [{:02x}:{:02x}.{}] (IO: {:#x}, MMIO: {:#x})",
            dev.vendor_id, dev.device_id, dev.bus, dev.slot, dev.func, self.io_base, self.mmio_base
        );

        // Step 4: Device Reset & Link Setup
        self.write_reg(REG_CTRL, CTRL_RST);
        for _ in 0..1000 { core::hint::spin_loop(); }

        self.write_reg(REG_CTRL, CTRL_SLU | CTRL_ASDE | CTRL_FD);

        // Disable and clear interrupts (polling mode for pure simplicity and rock-solid safety)
        self.write_reg(REG_IMC, 0xFFFF_FFFF);
        self.read_reg(REG_ICR);

        // Step 5: Read MAC Address
        self.read_mac_address();
        crate::serial_println!(
            "[e1000] Hardware MAC Address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac[0], self.mac[1], self.mac[2], self.mac[3], self.mac[4], self.mac[5]
        );

        // Step 6: Initialize Receive (RX) Descriptor Ring & Buffers
        crate::serial_println!("[e1000] Allocating RX descriptor ring ({} entries)...", RX_DESC_COUNT);
        let mut rx_descs_vec: Vec<RxDesc> = Vec::with_capacity(RX_DESC_COUNT);
        for _ in 0..RX_DESC_COUNT {
            rx_descs_vec.push(RxDesc::default());
        }
        let mut rx_descs = rx_descs_vec.into_boxed_slice();
        let mut rx_buffers = Vec::with_capacity(RX_DESC_COUNT);

        crate::serial_println!("[e1000] Allocating {} RX packet buffers ({} bytes each)...", RX_DESC_COUNT, PACKET_BUFFER_SIZE);
        for i in 0..RX_DESC_COUNT {
            let buffer = Box::new([0u8; PACKET_BUFFER_SIZE]);
            let phys_addr = virt_to_phys(buffer.as_ptr() as u64).unwrap_or(buffer.as_ptr() as u64);

            rx_descs[i].buffer_addr = phys_addr;
            rx_descs[i].status = 0;
            rx_buffers.push(buffer);
        }

        let rx_ring_phys = virt_to_phys(rx_descs.as_ptr() as u64).unwrap_or(rx_descs.as_ptr() as u64);
        self.write_reg(REG_RDBAL, rx_ring_phys as u32);
        self.write_reg(REG_RDBAH, (rx_ring_phys >> 32) as u32);
        self.write_reg(REG_RDLEN, (RX_DESC_COUNT * core::mem::size_of::<RxDesc>()) as u32);
        self.write_reg(REG_RDH, 0);
        self.write_reg(REG_RDT, (RX_DESC_COUNT - 1) as u32);

        // Enable Receiver: Broadcast Accept, Strip CRC, 2048B buffer
        self.write_reg(REG_RCTL, RCTL_EN | RCTL_BAM | RCTL_SECRC | RCTL_BSIZE_2048);

        self.rx_descs = Some(rx_descs);
        self.rx_buffers = rx_buffers;
        self.rx_cur = 0;

        // Step 7: Initialize Transmit (TX) Descriptor Ring & Buffers
        crate::serial_println!("[e1000] Allocating TX descriptor ring ({} entries)...", TX_DESC_COUNT);
        let mut tx_descs_vec: Vec<TxDesc> = Vec::with_capacity(TX_DESC_COUNT);
        for _ in 0..TX_DESC_COUNT {
            tx_descs_vec.push(TxDesc::default());
        }
        let mut tx_descs = tx_descs_vec.into_boxed_slice();
        let mut tx_buffers = Vec::with_capacity(TX_DESC_COUNT);

        crate::serial_println!("[e1000] Allocating {} TX packet buffers ({} bytes each)...", TX_DESC_COUNT, PACKET_BUFFER_SIZE);
        for i in 0..TX_DESC_COUNT {
            let buffer = Box::new([0u8; PACKET_BUFFER_SIZE]);
            let phys_addr = virt_to_phys(buffer.as_ptr() as u64).unwrap_or(buffer.as_ptr() as u64);

            tx_descs[i].buffer_addr = phys_addr;
            tx_descs[i].status = 1; // Initially marked as DD (available)
            tx_buffers.push(buffer);
        }

        let tx_ring_phys = virt_to_phys(tx_descs.as_ptr() as u64).unwrap_or(tx_descs.as_ptr() as u64);
        self.write_reg(REG_TDBAL, tx_ring_phys as u32);
        self.write_reg(REG_TDBAH, (tx_ring_phys >> 32) as u32);
        self.write_reg(REG_TDLEN, (TX_DESC_COUNT * core::mem::size_of::<TxDesc>()) as u32);
        self.write_reg(REG_TDH, 0);
        self.write_reg(REG_TDT, 0);

        // Standard IPG values for Gigabit
        self.write_reg(REG_TIPG, 10 | (10 << 10) | (10 << 20));

        // Enable Transmitter: Pad Short Packets, Standard Collision Settings
        self.write_reg(REG_TCTL, TCTL_EN | TCTL_PSP | TCTL_CT | TCTL_COLD);

        self.tx_descs = Some(tx_descs);
        self.tx_buffers = tx_buffers;
        self.tx_cur = 0;

        self.is_initialized = true;
        crate::serial_println!("[e1000] Network Controller successfully initialized and active.");
        true
    }

    /// Transmits an Ethernet frame over the network.
    pub fn send_packet(&mut self, packet: &[u8]) -> bool {
        if !self.is_initialized || packet.is_empty() || packet.len() > PACKET_BUFFER_SIZE {
            return false;
        }

        let tx_descs = match self.tx_descs.as_mut() {
            Some(d) => d,
            None => return false,
        };

        let cur = self.tx_cur;

        // Copy packet data to current TX buffer
        self.tx_buffers[cur][..packet.len()].copy_from_slice(packet);

        // Configure descriptor: EOP (End of Packet) | IFCS (Insert FCS) | RS (Report Status)
        tx_descs[cur].length = packet.len() as u16;
        tx_descs[cur].cmd = (1 << 0) | (1 << 1) | (1 << 3);
        tx_descs[cur].status = 0;

        self.tx_cur = (cur + 1) % TX_DESC_COUNT;

        // Notify controller of new transmit descriptor
        self.write_reg(REG_TDT, self.tx_cur as u32);

        self.tx_packets_count += 1;
        self.tx_bytes_count += packet.len() as u64;

        true
    }

    /// Polls for an incoming Ethernet packet from the RX ring.
    pub fn receive_packet(&mut self) -> Option<Vec<u8>> {
        if !self.is_initialized {
            return None;
        }

        let rx_descs = match self.rx_descs.as_mut() {
            Some(d) => d,
            None => return None,
        };

        let cur = self.rx_cur;

        // Check if current descriptor is done (DD bit 0)
        if (rx_descs[cur].status & 1) == 0 {
            return None;
        }

        let len = rx_descs[cur].length as usize;
        let mut packet = vec![0u8; len];
        packet.copy_from_slice(&self.rx_buffers[cur][..len]);

        // Reset descriptor status and advance tail
        rx_descs[cur].status = 0;
        self.write_reg(REG_RDT, cur as u32);
        self.rx_cur = (cur + 1) % RX_DESC_COUNT;

        self.rx_packets_count += 1;
        self.rx_bytes_count += len as u64;

        Some(packet)
    }
}

/// Global Intel e1000 driver instance protected by Spinlock.
pub static E1000_DEVICE: Spinlock<E1000> = Spinlock::new(E1000::new());

/// Initializes the global Intel e1000 network driver.
pub fn init() -> bool {
    let mut nic = E1000_DEVICE.lock();
    nic.init()
}
