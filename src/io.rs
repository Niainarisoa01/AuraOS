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

/// Writes a 16-bit word to the specified I/O port.
#[allow(dead_code)]
#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Reads a 16-bit word from the specified I/O port.
#[allow(dead_code)]
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        core::arch::asm!(
            "in ax, dx",
            out("ax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

/// Writes a 32-bit double word to the specified I/O port.
#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Reads a 32-bit double word from the specified I/O port.
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {
        core::arch::asm!(
            "in eax, dx",
            out("eax") value,
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

