/// ============================================================================
/// x86 Port-Mapped I/O Primitives
/// ============================================================================
///
/// The x86 architecture features a separate I/O address space from regular RAM.
/// Low-level peripherals (such as the 8259 PIC, PS/2 Keyboard, Serial Port,
/// and RTC Clock) are accessed using the CPU assembly instructions `in` and `out`.

/// Writes an 8-bit byte to the specified I/O port.
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Reads an 8-bit byte from the specified I/O port.
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

/// Minimal I/O delay (~1 to 4 microseconds).
///
/// Writing to port 0x80 (traditionally reserved for POST diagnostic cards)
/// gives slow hardware devices (like the legacy 8259 PIC) time to settle
/// without any unwanted side effects.
#[inline]
pub unsafe fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}
