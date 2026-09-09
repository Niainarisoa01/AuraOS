//! ============================================================================
//! PIT 8254 — Programmable Interval Timer (100 Hz Preemptive Tick)
//! ============================================================================
//!
//! The Intel 8254 PIT generates periodic hardware interrupts (IRQ 0) that drive
//! the kernel's preemptive scheduler. By default, the BIOS configures Channel 0
//! at ~18.2 Hz (~55ms per tick). We reprogram it to 100 Hz (10ms per tick) for
//! responsive multitasking and precise sleep timing.
//!
//! PIT I/O Ports:
//!   0x40 — Channel 0 Data (connected to IRQ 0)
//!   0x43 — Command/Mode Register
//!
//! Mode 3 (Square Wave Generator) is used for steady periodic interrupts.

use crate::arch::io;

/// PIT I/O port addresses
const PIT_CHANNEL0_DATA: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;

/// PIT oscillator frequency: 1,193,182 Hz (standard ISA clock)
const PIT_BASE_FREQUENCY: u32 = 1_193_182;

/// Target interrupt frequency in Hz
pub const TARGET_FREQUENCY: u32 = 100;

/// Calculated divisor for 100 Hz: 1_193_182 / 100 = 11_931
const DIVISOR: u16 = (PIT_BASE_FREQUENCY / TARGET_FREQUENCY) as u16;

/// PIT Command byte:
///   Bits 7-6: Channel 0 (00)
///   Bits 5-4: Access mode lobyte/hibyte (11)
///   Bits 3-1: Mode 3 — Square Wave Generator (011)
///   Bit 0:    Binary counting (0)
const COMMAND_BYTE: u8 = 0b00_11_011_0; // 0x36

/// Initialize the PIT Channel 0 at 100 Hz for preemptive scheduling.
///
/// After this call, IRQ 0 fires every 10ms instead of every 55ms.
pub fn init() {
    unsafe {
        // Send command: Channel 0, lobyte/hibyte, Mode 3, binary
        io::outb(PIT_COMMAND, COMMAND_BYTE);

        // Send divisor low byte, then high byte
        io::outb(PIT_CHANNEL0_DATA, (DIVISOR & 0xFF) as u8);
        io::outb(PIT_CHANNEL0_DATA, ((DIVISOR >> 8) & 0xFF) as u8);
    }

    crate::println!("[OK] PIT 8254  : Reprogrammed to {} Hz ({} us per tick, divisor={}).",
        TARGET_FREQUENCY, 1_000_000 / TARGET_FREQUENCY, DIVISOR);
    crate::serial_println!("[OK] PIT 8254: {}Hz (divisor={})", TARGET_FREQUENCY, DIVISOR);
}

/// Returns the tick interval in milliseconds.
#[allow(dead_code)]
pub const fn tick_interval_ms() -> u32 {
    1000 / TARGET_FREQUENCY
}

/// Returns the actual frequency achieved by the PIT.
#[allow(dead_code)]
pub const fn actual_frequency() -> u32 {
    PIT_BASE_FREQUENCY / DIVISOR as u32
}
