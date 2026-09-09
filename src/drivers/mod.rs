//! ============================================================================
//! Hardware Peripheral Drivers Layer
//! ============================================================================
//!
//! Houses all hardware peripheral device drivers:
//! - Display: VGA 80x25 Text Mode Buffer
//! - Serial: 16550 UART COM1 Debug Port
//! - Input: PS/2 Keyboard Controller
//! - Real-Time Clock: CMOS RTC
//! - Interconnect: PCI Bus Hardware Scanner
//! - Storage: ATA / IDE PIO Hard Disk Controller

pub mod vga;
pub mod serial;
pub mod keyboard;
pub mod cmos;
pub mod pci;
pub mod ata;
pub mod framebuffer;
pub mod bga;
pub mod mouse;
pub mod pit;
