//! ============================================================================
//! VGA Text Mode Driver (80x25 Memory Mapped at 0xb8000)
//! ============================================================================
//!
//! Hardware text buffer driver managing characters, 16-color attributes,
//! hardware scrolling, backspace handling, and global formatted print macros.

use core::fmt;
use core::ptr::write_volatile;
use crate::sync::Spinlock;

/// Standard 16 colors for the x86 VGA text palette.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// Combines foreground text color and background color into a single byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub const fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

/// Representation of a single VGA text character cell (2 bytes: ASCII + Color attribute).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

/// Standard VGA text mode screen dimensions.
const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

/// VGA memory buffer pointing to physical video RAM at `0xb8000`.
#[repr(transparent)]
struct Buffer {
    chars: [[ScreenChar; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// VGA text writer managing the cursor position, color attributes, and scrolling.
pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: *mut Buffer,
}

unsafe impl Send for Writer {}

impl Writer {
    /// Safe runtime access to the volatile hardware video buffer.
    #[inline]
    fn buffer_mut(&mut self) -> &mut Buffer {
        unsafe { &mut *self.buffer }
    }

    /// Writes a single byte (ASCII character or newline).
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;
                let color_code = self.color_code;

                unsafe {
                    write_volatile(
                        &mut self.buffer_mut().chars[row][col],
                        ScreenChar {
                            ascii_character: byte,
                            color_code,
                        },
                    );
                }
                self.column_position += 1;
            }
        }
    }

    /// Writes an entire string slice to the screen.
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
    }

    /// Shifts all screen lines up by one row (hardware auto-scrolling).
    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = unsafe {
                    core::ptr::read_volatile(&self.buffer_mut().chars[row][col])
                };
                unsafe {
                    write_volatile(&mut self.buffer_mut().chars[row - 1][col], character);
                }
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    /// Clears a specific row by filling it with blank space characters.
    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            unsafe {
                write_volatile(&mut self.buffer_mut().chars[row][col], blank);
            }
        }
    }

    /// Clears the entire VGA screen and resets the cursor to top-left.
    pub fn clear_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.column_position = 0;
    }

    /// Erases the last printed character on the current row (Backspace key).
    pub fn backspace(&mut self) {
        if self.column_position > 0 {
            self.column_position -= 1;
            let row = BUFFER_HEIGHT - 1;
            let col = self.column_position;
            let blank = ScreenChar {
                ascii_character: b' ',
                color_code: self.color_code,
            };
            unsafe {
                write_volatile(&mut self.buffer_mut().chars[row][col], blank);
            }
        }
    }

    /// Updates the active foreground and background text color.
    #[allow(dead_code)]
    pub fn set_color(&mut self, foreground: Color, background: Color) {
        self.color_code = ColorCode::new(foreground, background);
    }
}

/// Implements `core::fmt::Write` so the `Writer` can use standard Rust formatting macros.
impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// Global singleton instance of the VGA text writer, synchronized with a Spinlock.
pub static WRITER: Spinlock<Writer> = Spinlock::new(Writer {
    column_position: 0,
    color_code: ColorCode::new(Color::LightCyan, Color::Black),
    buffer: 0xb8000 as *mut Buffer,
});

// ============================================================================
// Global Kernel Print Macros: print! and println!
// ============================================================================

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::drivers::vga::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    WRITER.lock().write_fmt(args).unwrap();
}

/// Erases the last character displayed on the screen.
pub fn backspace() {
    WRITER.lock().backspace();
}

/// Clears the entire VGA screen buffer.
pub fn clear_screen() {
    WRITER.lock().clear_screen();
}
