//! ============================================================================
//! Bochs BGA / VBE Video Driver (Bochs Graphics Adapter)
//! ============================================================================
//!
//! Provides high-resolution TrueColor graphical display capabilities (1024x768x32bpp)
//! on QEMU, Bochs, and VirtualBox emulators.
//!
//! Controls the hardware via standard Bochs VBE I/O ports:
//! - 0x01CE: Index register port
//! - 0x01CF: Data register port
//! - Linear Framebuffer (LFB) at physical address 0xFD000000 (BAR0)

use crate::arch::io::{inw, outw};
use crate::drivers::framebuffer::Canvas;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const VBE_DISPI_IOPORT_INDEX: u16 = 0x01CE;
const VBE_DISPI_IOPORT_DATA: u16 = 0x01CF;

const VBE_DISPI_INDEX_ID: u16 = 0;
const VBE_DISPI_INDEX_XRES: u16 = 1;
const VBE_DISPI_INDEX_YRES: u16 = 2;
const VBE_DISPI_INDEX_BPP: u16 = 3;
const VBE_DISPI_INDEX_ENABLE: u16 = 4;
#[allow(dead_code)]
const VBE_DISPI_INDEX_BANK: u16 = 5;

const VBE_DISPI_DISABLED: u16 = 0x00;
const VBE_DISPI_ENABLED: u16 = 0x01;
const VBE_DISPI_LFB_ENABLED: u16 = 0x40;

/// Default screen resolution
pub const SCREEN_WIDTH: usize = 1024;
pub const SCREEN_HEIGHT: usize = 768;
pub const SCREEN_BPP: usize = 32;

/// Physical address of the Bochs VGA Linear Framebuffer (default on QEMU x86_64)
const DEFAULT_LFB_PHYS_ADDR: u32 = 0xFD000000;

static LFB_BASE_ADDR: AtomicU32 = AtomicU32::new(DEFAULT_LFB_PHYS_ADDR);
static BGA_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Writes a 16-bit value to a specified BGA register.
fn bga_write(index: u16, data: u16) {
    unsafe {
        outw(VBE_DISPI_IOPORT_INDEX, index);
        outw(VBE_DISPI_IOPORT_DATA, data);
    }
}

/// Reads a 16-bit value from a specified BGA register.
fn bga_read(index: u16) -> u16 {
    unsafe {
        outw(VBE_DISPI_IOPORT_INDEX, index);
        inw(VBE_DISPI_IOPORT_DATA)
    }
}

/// Detects if the Bochs Graphics Adapter hardware is present.
pub fn is_available() -> bool {
    let id = bga_read(VBE_DISPI_INDEX_ID);
    // BGA IDs range from 0xB0C0 to 0xB0C6
    id >= 0xB0C0 && id <= 0xB0C6
}

/// Returns true if the BGA graphics mode is currently active.
pub fn is_active() -> bool {
    BGA_ACTIVE.load(Ordering::Relaxed)
}

/// Sets the base physical address of the Linear Framebuffer.
pub fn set_framebuffer_addr(addr: u32) {
    LFB_BASE_ADDR.store(addr, Ordering::Relaxed);
}

/// Returns the pointer to the active Linear Framebuffer in memory.
pub fn framebuffer_ptr() -> *mut u32 {
    LFB_BASE_ADDR.load(Ordering::Relaxed) as *mut u32
}

/// Initializes high-resolution 1024x768x32bpp TrueColor graphics mode.
pub fn init_graphics_mode(width: usize, height: usize) -> bool {
    if !is_available() {
        return false;
    }

    // Step 1: Ensure PCI MMIO address space (3 GiB .. 4 GiB) is mapped
    crate::memory::paging::map_mmio_pci_range();

    // Step 2: Attempt to find exact BAR0 from PCI bus scan if available
    let pci_devices = crate::drivers::pci::scan_pci_bus();
    for dev in pci_devices {
        if dev.vendor_id == 0x1234 && dev.device_id == 0x1111 {
            let bar0 = crate::drivers::pci::pci_read_config_u32(dev.bus, dev.slot, dev.func, 0x10);
            let phys_addr = bar0 & 0xFFF0_0000;
            if phys_addr != 0 {
                set_framebuffer_addr(phys_addr);
            }
            break;
        }
    }

    // Step 3: Configure Bochs BGA registers
    bga_write(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_DISABLED);
    bga_write(VBE_DISPI_INDEX_XRES, width as u16);
    bga_write(VBE_DISPI_INDEX_YRES, height as u16);
    bga_write(VBE_DISPI_INDEX_BPP, SCREEN_BPP as u16);
    bga_write(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_ENABLED | VBE_DISPI_LFB_ENABLED);

    BGA_ACTIVE.store(true, Ordering::Relaxed);
    true
}

/// Reverts the display adapter back to standard VGA text mode 80x25.
pub fn disable_graphics_mode() {
    bga_write(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_DISABLED);
    BGA_ACTIVE.store(false, Ordering::Relaxed);
}

/// Blits an in-memory 32-bit Canvas directly onto the physical screen framebuffer.
pub fn present(canvas: &Canvas) {
    if !is_active() {
        return;
    }

    let fb = framebuffer_ptr();
    let pixel_count = canvas.width.min(SCREEN_WIDTH) * canvas.height.min(SCREEN_HEIGHT);
    let src_slice = canvas.pixels.as_slice();

    unsafe {
        core::ptr::copy_nonoverlapping(src_slice.as_ptr(), fb, pixel_count.min(src_slice.len()));
    }
}
