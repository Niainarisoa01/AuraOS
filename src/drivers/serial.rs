//! ============================================================================
//! Serial Port Driver — 16550 UART (COM1) + Per-CPU Ring Buffers
//! ============================================================================
//!
//! The 16550 UART (Universal Asynchronous Receiver-Transmitter) is standard
//! on x86 PCs and QEMU. It transmits text data byte-by-byte over a serial line.
//!
//! This provides a robust debugging channel that works independently of the
//! video graphics hardware. In QEMU, COM1 can be piped directly to stdout
//! via `-serial stdio` or logged to a file.
//!
//! ## I1 — Per-CPU Buffered Serial Output
//!
//! To eliminate global lock contention across cores, each CPU writes formatted
//! output into its own local ring buffer. A BSP flusher drains all per-CPU
//! buffers to the physical UART in round-robin order. This preserves per-CPU
//! FIFO ordering (INV-S1) while allowing lock-free writes on the hot path.
//!
//! Exception/panic paths bypass the buffers and write directly to the UART
//! via `force_unlock` (INV-S2).

use core::fmt;
use core::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use crate::arch::io::{inb, outb};
use crate::sync::Spinlock;

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
            outb(self.port_base, 0x01); // Divisor LSB
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
/// After I1 migration, this lock is only taken by:
///   - `serial_flush_all()` for draining per-CPU buffers to the physical UART
///   - Exception/panic paths (`force_unlock` + direct write)
///   - `serial::init()` at boot (BSP-only)
///   - `receive_byte` callers (keyboard.rs, gui/mod.rs)
pub static SERIAL1: Spinlock<SerialPort> = Spinlock::new(SerialPort::new(COM1_BASE));

/// Initializes the primary COM1 serial port.
pub fn init() {
    SERIAL1.lock().init();
    crate::println!("[OK] Serial    : COM1 (UART 16550 at 0x3F8, 115200 baud) active.");
}

// ============================================================================
// Per-CPU Serial Ring Buffers (I1)
// ============================================================================

/// Size of each per-CPU ring buffer in bytes (8 KiB).
const SERIAL_BUF_SIZE: usize = 8192;

/// Per-CPU ring buffer for serial output. Each CPU writes into its own buffer
/// without taking the global `SERIAL1` lock. The BSP flusher drains these
/// buffers to the physical UART in round-robin order.
///
/// # Safety Invariants
/// - `head` and `tail` are always < SERIAL_BUF_SIZE (enforced by modular arithmetic).
/// - Only the owning CPU writes to `tail` and `data[tail]` (via `push_byte`).
/// - Only the BSP flusher reads from `head` and `data[head]` (via `pop_byte`).
/// - The `lock` is taken with interrupt masking to handle reentrancy on the same CPU
///   (e.g., timer ISR preempting a `serial_println!`).
pub struct PerCpuSerialBuffer {
    /// Ring buffer data. Uses UnsafeCell for interior mutability because
    /// push_byte needs to write through a shared reference (the static array
    /// is not mutable). Access is serialized by the per-buffer lock.
    data: core::cell::UnsafeCell<[u8; SERIAL_BUF_SIZE]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    /// Number of bytes dropped due to buffer overflow (diagnostic).
    dropped: AtomicUsize,
    /// Per-buffer lock for reentrancy and IRQ protection on the same CPU.
    lock: crate::sync::Spinlock<()>,
}

impl PerCpuSerialBuffer {
    const fn new() -> Self {
        PerCpuSerialBuffer {
            data: core::cell::UnsafeCell::new([0u8; SERIAL_BUF_SIZE]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            lock: crate::sync::Spinlock::new(()),
        }
    }

    /// Pushes a single byte into the ring buffer. If the buffer is full,
    /// the byte is discarded and `dropped` is incremented.
    ///
    /// # Safety
    /// The caller must hold the per-buffer lock (via `self.lock.lock()`).
    /// This uses `UnsafeCell` for interior mutability — safe because the lock
    /// ensures exclusive write access.
    fn push_byte(&self, byte: u8) {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % SERIAL_BUF_SIZE;
        if next_tail == self.head.load(Ordering::Acquire) {
            // Buffer full — drop the byte
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        // SAFETY: We hold the per-buffer lock, so no concurrent write to data[tail].
        // The BSP flusher only reads data[head], and head != tail (checked above).
        unsafe { (*self.data.get())[tail] = byte; }
        self.tail.store(next_tail, Ordering::Release);
    }

    /// Pops a single byte from the ring buffer. Returns `None` if empty.
    /// Called only by the BSP flusher.
    fn pop_byte(&self) -> Option<u8> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        // SAFETY: Only the BSP flusher calls pop_byte, so head is never
        // concurrently modified. The data at `head` was written by the owning
        // CPU and is visible because tail was stored with Release ordering.
        let byte = unsafe { (*self.data.get())[head] };
        self.head.store((head + 1) % SERIAL_BUF_SIZE, Ordering::Release);
        Some(byte)
    }

    /// Returns the number of bytes currently in the buffer.
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        if tail >= head { tail - head } else { SERIAL_BUF_SIZE - head + tail }
    }

    /// Returns true if the buffer is empty.
    fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    /// Returns and resets the dropped byte count.
    fn take_dropped(&self) -> usize {
        self.dropped.swap(0, Ordering::Relaxed)
    }
}

// SAFETY: PerCpuSerialBuffer is accessed with proper atomic ordering and per-buffer locking.
// UnsafeCell is only accessed under the per-buffer lock (writes) or by the BSP flusher (reads).
unsafe impl Sync for PerCpuSerialBuffer {}

/// Static array of per-CPU serial ring buffers, one per possible CPU.
static SERIAL_BUFFERS: [PerCpuSerialBuffer; crate::arch::smp::MAX_CPUS] =
    [const { PerCpuSerialBuffer::new() }; crate::arch::smp::MAX_CPUS];

/// Set to `true` once per-CPU buffers are ready (after serial init + SMP init).
/// Before this flag is set, `_print` falls back to direct SERIAL1 lock.
static PERCPU_SERIAL_READY: AtomicBool = AtomicBool::new(false);

/// Called by the BSP after serial and SMP initialization to enable per-CPU buffering.
pub fn enable_percpu_buffering() {
    PERCPU_SERIAL_READY.store(true, Ordering::Release);
}

/// Returns true if per-CPU serial buffering is active.
#[inline]
pub fn is_percpu_buffering_active() -> bool {
    PERCPU_SERIAL_READY.load(Ordering::Acquire)
}

/// Writes formatted arguments into the calling CPU's per-CPU serial ring buffer.
/// This is the hot path that replaces `SERIAL1.lock().write_fmt()`.
///
/// If per-CPU buffering is not yet active (early boot), falls back to direct
/// SERIAL1 lock for backward compatibility.
struct PerCpuSerialWriter {
    cpu_id: usize,
}

impl fmt::Write for PerCpuSerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let buf = &SERIAL_BUFFERS[self.cpu_id];
        // The per-buffer lock is held by the caller (_print).
        for byte in s.bytes() {
            if byte == b'\n' {
                buf.push_byte(b'\r');
            }
            buf.push_byte(byte);
        }
        Ok(())
    }
}

/// Drains all per-CPU serial buffers to the physical UART.
/// Called by the BSP flusher (e.g. LAPIC timer tick or HLT loop).
/// Takes the `SERIAL1` lock via try_lock() once and drains
/// all buffers in round-robin order (one batch per CPU to maintain per-CPU FIFO).
///
/// This is the only function (aside from panic/exception paths) that takes
/// the `SERIAL1` lock after I1 migration.
pub fn serial_flush_all() {
    if !is_percpu_buffering_active() {
        return;
    }

    let online = crate::arch::smp::cpu_count().clamp(1, crate::arch::smp::MAX_CPUS);

    // Quick check: anything to flush?
    let mut any_pending = false;
    for i in 0..online {
        if !SERIAL_BUFFERS[i].is_empty() {
            any_pending = true;
            break;
        }
    }
    if !any_pending {
        return;
    }

    // Try to acquire the physical UART lock. If held (e.g. nested flush or exception), skip this tick.
    let Some(mut serial) = SERIAL1.try_lock() else {
        return;
    };

    for cpu_id in 0..online {
        let buf = &SERIAL_BUFFERS[cpu_id];

        // Drain dropped count first
        let dropped = buf.take_dropped();
        if dropped > 0 {
            let msg = b"[DROPPED]\r\n";
            for &b in msg {
                serial.send_byte(b);
            }
        }

        // Drain buffer contents
        while let Some(byte) = buf.pop_byte() {
            serial.send_byte(byte);
        }
    }
}

/// Returns the number of bytes currently buffered for diagnostic purposes.
#[allow(dead_code)]
pub fn serial_buffer_pending(cpu_id: usize) -> usize {
    if cpu_id < crate::arch::smp::MAX_CPUS {
        SERIAL_BUFFERS[cpu_id].len()
    } else {
        0
    }
}

/// Returns true if the SERIAL1 global lock is currently held.
/// Used by tests to verify that per-CPU writes don't contend on the global lock.
#[allow(dead_code)]
pub fn serial_global_lock_held() -> bool {
    // Try to acquire; if we can, it wasn't held, release it immediately
    if let Some(_guard) = SERIAL1.try_lock() {
        false
    } else {
        true
    }
}

// ============================================================================
// Serial Print Macros (serial_print! and serial_println!)
// ============================================================================

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::drivers::serial::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;

    if is_percpu_buffering_active() {
        // I1 hot path: write into per-CPU ring buffer (no global lock, IRQ-safe)
        let cpu_id = crate::arch::smp::current_cpu();
        let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };
        let buf = &SERIAL_BUFFERS[cpu_id];
        let _guard = buf.lock.lock();
        let mut writer = PerCpuSerialWriter { cpu_id };
        let _ = writer.write_fmt(args);
    } else {
        // Early boot fallback: direct UART access under global lock
        SERIAL1.lock().write_fmt(args).unwrap();
    }
}

/// Reads a byte from the serial port without taking the global lock.
/// Reading the Line Status Register (port+5) and data register (port) is safe
/// to do concurrently with writes because the UART has separate receive and
/// transmit holding registers.
///
/// # Safety
/// This accesses I/O ports directly. It is safe because:
/// - The UART receive buffer is independent of the transmit buffer.
/// - Only one caller (BSP keyboard polling) reads at a time.
pub fn receive_byte_lockfree() -> Option<u8> {
    unsafe {
        if (inb(COM1_BASE + 5) & 1) != 0 {
            Some(inb(COM1_BASE))
        } else {
            None
        }
    }
}
