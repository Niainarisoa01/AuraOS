//! ============================================================================
//! PS/2 Mouse Driver (Intel 8042 Auxiliary Device on IRQ 12)
//! ============================================================================
//!
//! Handles standard PS/2 3-byte movement packets, tracking absolute cursor
//! coordinates, button clicks (left, right, middle), and screen boundary clamping.

use crate::arch::io::{inb, outb, io_wait};
use crate::sync::Spinlock;

const PORT_DATA: u16 = 0x60;
const PORT_STATUS: u16 = 0x64;
const PORT_CMD: u16 = 0x64;

/// Current dynamic mouse state
#[derive(Debug, Clone, Copy)]
pub struct MouseState {
    pub x: usize,
    pub y: usize,
    pub left_button: bool,
    pub right_button: bool,
    pub middle_button: bool,
}

pub static MOUSE: Spinlock<MouseState> = Spinlock::new(MouseState {
    x: 512, // Initial center on 1024x768
    y: 384,
    left_button: false,
    right_button: false,
    middle_button: false,
});

/// Packet reception buffer
struct MouseBuffer {
    bytes: [u8; 3],
    index: usize,
}

static PACKET_BUFFER: Spinlock<MouseBuffer> = Spinlock::new(MouseBuffer {
    bytes: [0; 3],
    index: 0,
});

/// Waits until the 8042 input buffer is clear and ready to accept a write.
fn wait_input_ready() {
    for _ in 0..100_000 {
        if unsafe { inb(PORT_STATUS) } & 0x02 == 0 {
            return;
        }
        unsafe { io_wait() };
    }
}

/// Waits until the 8042 output buffer has data ready to read.
fn wait_output_ready() {
    for _ in 0..100_000 {
        if unsafe { inb(PORT_STATUS) } & 0x01 != 0 {
            return;
        }
        unsafe { io_wait() };
    }
}

/// Sends a command byte to the mouse auxiliary port.
fn write_mouse_cmd(cmd: u8) {
    wait_input_ready();
    unsafe { outb(PORT_CMD, 0xD4) };
    wait_input_ready();
    unsafe { outb(PORT_DATA, cmd) };
}

/// Reads a byte from the 8042 data port.
fn read_data() -> u8 {
    wait_output_ready();
    unsafe { inb(PORT_DATA) }
}

/// Initializes the PS/2 mouse hardware and enables data streaming.
pub fn init() {
    // Step 1: Enable the auxiliary device on the 8042 controller
    wait_input_ready();
    unsafe { outb(PORT_CMD, 0xA8) };

    // Step 2: Read controller command byte
    wait_input_ready();
    unsafe { outb(PORT_CMD, 0x20) };
    let mut status = read_data();

    // Step 3: Enable IRQ 12 (bit 1) and enable clock (clear bit 5)
    status |= 0x02;
    status &= !0x20;

    wait_input_ready();
    unsafe { outb(PORT_CMD, 0x60) };
    wait_input_ready();
    unsafe { outb(PORT_DATA, status) };

    // Step 4: Set mouse to default sampling parameters
    write_mouse_cmd(0xF6);
    let _ = read_data(); // ACK (0xFA)

    // Step 5: Enable mouse data streaming
    write_mouse_cmd(0xF4);
    let _ = read_data(); // ACK (0xFA)
}

/// Handles an incoming byte from the PS/2 mouse interrupt (IRQ 12).
pub fn handle_interrupt() {
    let status = unsafe { inb(PORT_STATUS) };
    if (status & 0x01) == 0 {
        return;
    }

    let byte = unsafe { inb(PORT_DATA) };
    let mut buf = PACKET_BUFFER.lock();

    // Packet synchronization: Byte 0 must always have bit 3 set to 1
    if buf.index == 0 && (byte & 0x08) == 0 {
        return; // Desynchronized packet, discard byte
    }

    let idx = buf.index;
    buf.bytes[idx] = byte;
    buf.index += 1;

    if buf.index == 3 {
        buf.index = 0;

        let b0 = buf.bytes[0];
        let b1 = buf.bytes[1];
        let b2 = buf.bytes[2];

        // Discard packet if overflow flags are set
        if (b0 & 0x80) != 0 || (b0 & 0x40) != 0 {
            return;
        }

        let left = (b0 & 0x01) != 0;
        let right = (b0 & 0x02) != 0;
        let middle = (b0 & 0x04) != 0;

        // Compute delta X with sign extension
        let mut dx = b1 as i32;
        if (b0 & 0x10) != 0 {
            dx |= !0xFF;
        }

        // Compute delta Y with sign extension
        let mut dy = b2 as i32;
        if (b0 & 0x20) != 0 {
            dy |= !0xFF;
        }

        let mut mouse = MOUSE.lock();
        mouse.left_button = left;
        mouse.right_button = right;
        mouse.middle_button = middle;

        // Apply movement with screen clamping (1024x768)
        let new_x = (mouse.x as i32 + dx).clamp(0, 1023) as usize;
        // Invert Y delta (PS/2 reports positive Y upwards, screen coordinates go downwards)
        let new_y = (mouse.y as i32 - dy).clamp(0, 767) as usize;

        mouse.x = new_x;
        mouse.y = new_y;
    }
}

/// Returns a copy of the current mouse state.
pub fn get_state() -> MouseState {
    *MOUSE.lock()
}
