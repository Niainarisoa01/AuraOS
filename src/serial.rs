/// ============================================================================
/// Serial Port Driver — 16550 UART (COM1)
/// ============================================================================
///
/// The 16550 UART (Universal Asynchronous Receiver-Transmitter) is standard
/// on x86 PCs and QEMU. It transmits text data byte-by-byte over a serial line.
///
/// This provides a robust debugging channel that works independently of the
/// video graphics hardware. In QEMU, COM1 can be piped directly to stdout
/// via `-serial stdio` or logged to a file.

use core::fmt;
use crate::io::{inb, outb};
use crate::vga_buffer::Spinlock;

/// Standard I/O port address for COM1.
pub const COM1_BASE: u16 = 0x3F8;

/// 16550 UART Controller driver for an individual COM port.
pub struct SerialPort {
    port_base: u16,
}

impl SerialPort {
    /// Creates a new serial port interface at the given base I/O port.
    pub const fn new(port_base: u16) -> Self {
        SerialPort { port_base }
    }

    /// Initializes the 16550 UART with 115200 baud, 8N1 (8 data bits, no parity, 1 stop bit).
    pub fn init(&mut self) {
        unsafe {
            // 1. Disable all interrupts while configuring
            outb(self.port_base + 1, 0x00);

            // 2. Enable DLAB (Divisor Latch Access Bit) to set baud rate
            outb(self.port_base + 3, 0x80);

            // 3. Set divisor to 1 (115200 baud):
            //    Base clock is 1.8432 MHz / 16 = 115200 Hz. Divisor = 1.
            outb(self.port_base + 0, 0x01); // Divisor LSB
            outb(self.port_base + 1, 0x00); // Divisor MSB

            // 4. Line Control: 8 data bits, no parity, 1 stop bit (8N1)
            outb(self.port_base + 3, 0x03);

            // 5. FIFO Control: Enable FIFO, clear transmit/receive queues, 14-byte threshold
            outb(self.port_base + 2, 0xC7);

            // 6. Modem Control: RTS/DSR set, auxiliary output 2 enabled (0x0B)
            outb(self.port_base + 4, 0x0B);
        }
    }

    /// Returns true if the Transmit Holding Register is empty and ready to accept a new byte.
    #[inline]
    fn is_transmit_empty(&self) -> bool {
        unsafe { (inb(self.port_base + 5) & 0x20) != 0 }
    }

    /// Sends a single byte over the serial port, blocking until the transmitter is ready.
    pub fn send_byte(&mut self, byte: u8) {
        while !self.is_transmit_empty() {
            core::hint::spin_loop();
        }
        unsafe {
            outb(self.port_base, byte);
        }
    }

    /// Sends a string slice over the serial port.
    /// Automatically converts `\n` to `\r\n` for terminal compatibility.
    pub fn send_string(&mut self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.send_byte(b'\r');
            }
            self.send_byte(byte);
        }
    }

    /// Checks if a byte has been received and is waiting in the buffer.
    #[allow(dead_code)]
    pub fn has_received(&self) -> bool {
        unsafe { (inb(self.port_base + 5) & 1) != 0 }
    }

    /// Reads a received byte from the serial port, or returns None if no data is available.
    #[allow(dead_code)]
    pub fn receive_byte(&self) -> Option<u8> {
        if self.has_received() {
            Some(unsafe { inb(self.port_base) })
        } else {
            None
        }
    }
}

/// Implements `core::fmt::Write` to allow formatted output (`write!`, `serial_print!`).
impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.send_string(s);
        Ok(())
    }
}

/// Global singleton instance of the COM1 serial port, protected by a Spinlock.
pub static SERIAL1: Spinlock<SerialPort> = Spinlock::new(SerialPort::new(COM1_BASE));

/// Initializes the primary COM1 serial port.
pub fn init() {
    SERIAL1.lock().init();
    crate::println!("[OK] Serial    : COM1 (UART 16550 at 0x3F8, 115200 baud) active.");
}

// ============================================================================
// Serial Print Macros (serial_print! and serial_println!)
// ============================================================================

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::serial::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    SERIAL1.lock().write_fmt(args).unwrap();
}
