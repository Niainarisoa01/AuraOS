//! ============================================================================
//! Local APIC (Advanced Programmable Interrupt Controller) Subsystem
//! ============================================================================
//!
//! Provides bare-metal configuration of the per-CPU Local APIC (LAPIC):
//!   - MSR `IA32_APIC_BASE` (0x1B) configuration and global enablement
//!   - Memory-Mapped I/O register access (default base: 0xFEE00000)
//!   - Software APIC activation via Spurious Vector Register (SVR)
//!   - End-of-Interrupt (EOI) signaling
//!   - Local APIC ID, Version, and Task Priority (TPR) inspection

#![allow(dead_code)]

use crate::sync::Spinlock;

/// IA32_APIC_BASE MSR address.
pub const MSR_APIC_BASE: u32 = 0x001B;
pub const MSR_APIC_BASE_ENABLE: u64 = 1 << 11; // Bit 11: Global APIC Enable
pub const MSR_APIC_BASE_BSP: u64 = 1 << 8;    // Bit 8: Bootstrap Processor

// Local APIC Register Offsets (Memory Mapped, 4-byte aligned on 16-byte boundaries)
pub const REG_LAPIC_ID: u32 = 0x0020;        // Local APIC ID
pub const REG_LAPIC_VER: u32 = 0x0030;       // Local APIC Version
pub const REG_TPR: u32 = 0x0080;             // Task Priority Register
pub const REG_APR: u32 = 0x0090;             // Arbitration Priority Register
pub const REG_PPR: u32 = 0x00A0;             // Processor Priority Register
pub const REG_EOI: u32 = 0x00B0;             // End of Interrupt Register
pub const REG_LDR: u32 = 0x00D0;             // Logical Destination Register
pub const REG_DFR: u32 = 0x00E0;             // Destination Format Register
pub const REG_SVR: u32 = 0x00F0;             // Spurious Interrupt Vector Register
pub const REG_ESR: u32 = 0x0280;             // Error Status Register
pub const REG_ICR_LOW: u32 = 0x0300;         // Interrupt Command Register (0-31)
pub const REG_ICR_HIGH: u32 = 0x0310;        // Interrupt Command Register (32-63)
pub const REG_LVT_TIMER: u32 = 0x0320;       // LVT Timer Register
pub const REG_LVT_LINT0: u32 = 0x0350;       // LVT LINT0 Register
pub const REG_LVT_LINT1: u32 = 0x0360;       // LVT LINT1 Register
pub const REG_LVT_ERROR: u32 = 0x0370;       // LVT Error Register
pub const REG_TIMER_INIT: u32 = 0x0380;      // Timer Initial Count Register
pub const REG_TIMER_CUR: u32 = 0x0390;       // Timer Current Count Register
pub const REG_TIMER_DIV: u32 = 0x03E0;       // Timer Divide Configuration Register

/// Spurious Interrupt Vector Register bits
pub const SVR_APIC_SOFTWARE_ENABLE: u32 = 1 << 8; // Bit 8: APIC Software Enable
pub const SVR_SPURIOUS_VECTOR: u32 = 0xFF;        // Vector 255 (standard spurious vector)

pub use crate::arch::msr::{rdmsr, wrmsr};

/// Local APIC Controller Instance.
pub struct LocalApic {
    pub base_addr: u64,
    pub is_enabled: bool,
    pub apic_id: u8,
    pub version: u8,
}

impl LocalApic {
    pub const fn new() -> Self {
        LocalApic {
            base_addr: 0xFEE00000,
            is_enabled: false,
            apic_id: 0,
            version: 0,
        }
    }

    /// Reads a 32-bit register from the Local APIC MMIO region.
    #[inline]
    pub fn read_reg(&self, reg: u32) -> u32 {
        if self.base_addr == 0 {
            return 0;
        }
        unsafe {
            core::ptr::read_volatile((self.base_addr + reg as u64) as *const u32)
        }
    }

    /// Writes a 32-bit register into the Local APIC MMIO region.
    #[inline]
    pub fn write_reg(&self, reg: u32, value: u32) {
        if self.base_addr == 0 {
            return;
        }
        unsafe {
            core::ptr::write_volatile((self.base_addr + reg as u64) as *mut u32, value);
        }
    }

    /// Signals End of Interrupt (EOI) to the Local APIC.
    #[inline]
    pub fn send_eoi(&self) {
        self.write_reg(REG_EOI, 0);
    }
}

pub static LOCAL_APIC: Spinlock<LocalApic> = Spinlock::new(LocalApic::new());

/// Initializes the Local APIC for the current CPU core.
pub fn init() -> bool {
    let mut lapic = LOCAL_APIC.lock();

    // 1. Verify CPUID APIC support
    let cpu_info = crate::arch::cpuid::get_cpu_info();
    if !cpu_info.has_apic {
        crate::serial_println!("[LAPIC] Error: Processor does not support Local APIC.");
        return false;
    }

    // 2. Query IA32_APIC_BASE MSR to determine physical MMIO base address
    let apic_base_msr = unsafe { rdmsr(MSR_APIC_BASE) };
    let base_phys = apic_base_msr & 0x000F_FFFF_FFFF_F000;
    lapic.base_addr = if base_phys != 0 { base_phys } else { 0xFEE00000 };

    // 3. Ensure global APIC enable bit (bit 11) is set in MSR
    if (apic_base_msr & MSR_APIC_BASE_ENABLE) == 0 {
        unsafe {
            wrmsr(MSR_APIC_BASE, apic_base_msr | MSR_APIC_BASE_ENABLE);
        }
    }

    // 4. Read initial hardware ID and Version
    lapic.apic_id = ((lapic.read_reg(REG_LAPIC_ID) >> 24) & 0xFF) as u8;
    lapic.version = (lapic.read_reg(REG_LAPIC_VER) & 0xFF) as u8;

    // 5. Software Enable Local APIC via Spurious Interrupt Vector Register (SVR)
    // Vector 0xFF with bit 8 (APIC Software Enable) set
    lapic.write_reg(REG_SVR, SVR_APIC_SOFTWARE_ENABLE | SVR_SPURIOUS_VECTOR);

    // 6. Clear Task Priority Register (TPR = 0) to accept all interrupt priorities
    lapic.write_reg(REG_TPR, 0);

    // 7. Clear any pending errors
    lapic.write_reg(REG_ESR, 0);
    lapic.write_reg(REG_ESR, 0);

    // 8. Send an initial EOI to clear any lingering in-service interrupts
    lapic.send_eoi();

    lapic.is_enabled = true;

    crate::serial_println!(
        "[LAPIC] Initialized at {:#x}. ID: {}, Version: {:#x}, SVR: {:#x}",
        lapic.base_addr, lapic.apic_id, lapic.version, lapic.read_reg(REG_SVR)
    );

    true
}

/// Standard interrupt vector allocated for the Local APIC periodic timer.
pub const LAPIC_TIMER_VECTOR: u8 = 0x40; // Vector 64

/// Helper to send an End of Interrupt (EOI) from interrupt service routines.
#[inline]
pub fn send_eoi() {
    unsafe {
        core::ptr::write_volatile(0xFEE0_00B0 as *mut u32, 0);
    }
}

/// Initializes the Local APIC Timer on the calling CPU core in Periodic mode.
///   - `vector`: The interrupt vector to trigger on timer expiry (e.g. 0x40)
///   - `initial_count`: Initial counter reload value
pub fn init_timer(vector: u8, initial_count: u32) {
    let lapic_base = 0xFEE0_0000u64;
    unsafe {
        // 1. Configure Divider to divide bus clock by 16 (0x03)
        let div_addr = (lapic_base + REG_TIMER_DIV as u64) as *mut u32;
        core::ptr::write_volatile(div_addr, 0x03);

        // 2. Configure LVT Timer register: Periodic mode (bit 17 = 1), vector in [7:0], unmasked (bit 16 = 0)
        let lvt_addr = (lapic_base + REG_LVT_TIMER as u64) as *mut u32;
        core::ptr::write_volatile(lvt_addr, (1 << 17) | (vector as u32));

        // 3. Set Initial Count register to start countdown
        let init_addr = (lapic_base + REG_TIMER_INIT as u64) as *mut u32;
        core::ptr::write_volatile(init_addr, initial_count);
    }
}

