/// ============================================================================
/// PS/2 Keyboard Driver (Scancode Set 1)
/// ============================================================================
///
/// The PS/2 keyboard controller (Intel 8042) triggers an IRQ 1 interrupt
/// whenever a key is pressed or released.
///
/// When a key is pressed:  A "Make Code" is sent on port 0x60.
/// When a key is released: A "Break Code" (Make Code | 0x80) is sent.

use core::sync::atomic::{AtomicBool, Ordering};
use crate::io::inb;

const KEYBOARD_DATA_PORT: u16 = 0x60;

/// Tracks whether Shift (Left or Right) is currently held down.
static SHIFT_ACTIVE: AtomicBool = AtomicBool::new(false);

/// PS/2 (Set 1) scancode lookup table to lowercase ASCII characters.
static SCANCODE_LOWER: [u8; 58] = [
    0,    27,  b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', 8,   // 0x00 - 0x0E (8 = Backspace)
    b'\t', b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n',    // 0x0F - 0x1C (0x1C = Enter)
    0,    b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`',           // 0x1D - 0x29
    0,    b'\\', b'z', b'x', b'c', b'v', b'b', b'n', b'm', b',', b'.', b'/', 0,              // 0x2A - 0x35
    b'*', 0,   b' ',                                                                           // 0x36 - 0x39 (0x39 = Space)
];

/// PS/2 (Set 1) scancode lookup table to uppercase / shifted ASCII characters.
static SCANCODE_UPPER: [u8; 58] = [
    0,    27,  b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_', b'+', 8,   // 0x00 - 0x0E
    b'\t', b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}', b'\n',    // 0x0F - 0x1C
    0,    b'A', b'S', b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':', b'"', b'~',           // 0x1D - 0x29
    0,    b'|', b'Z', b'X', b'C', b'V', b'B', b'N', b'M', b'<', b'>', b'?', 0,              // 0x2A - 0x35
    b'*', 0,   b' ',                                                                           // 0x36 - 0x39
];

/// Handles a keyboard keystroke on IRQ 1 and forwards characters to the Shell.
pub fn handle_interrupt() {
    let scancode = unsafe { inb(KEYBOARD_DATA_PORT) };

    match scancode {
        // Left or Right Shift pressed
        0x2A | 0x36 => {
            SHIFT_ACTIVE.store(true, Ordering::Relaxed);
        }
        // Left or Right Shift released (Make Code | 0x80)
        0xAA | 0xB6 => {
            SHIFT_ACTIVE.store(false, Ordering::Relaxed);
        }
        // Key release event (bit 7 set) - ignore for now
        code if code & 0x80 != 0 => {}
        // Valid key press (Make Code)
        code => {
            let index = code as usize;
            if index < SCANCODE_LOWER.len() {
                let is_shift = SHIFT_ACTIVE.load(Ordering::Relaxed);
                let ascii = if is_shift {
                    SCANCODE_UPPER[index]
                } else {
                    SCANCODE_LOWER[index]
                };

                match ascii {
                    // Backspace
                    8 => {
                        crate::shell::SHELL.lock().backspace();
                    }
                    // Enter key (submit command)
                    b'\n' => {
                        crate::shell::SHELL.lock().enter();
                    }
                    // Printable ASCII character
                    0x20..=0x7E => {
                        crate::shell::SHELL.lock().push_char(ascii);
                    }
                    _ => {}
                }
            }
        }
    }
}
