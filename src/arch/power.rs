//! ============================================================================
//! Hardware Power Management Subsystem (ACPI Shutdown & Reset)
//! ============================================================================
//!
//! Handles clean system shutdown and reboot across physical hardware and emulators:
//!   - ACPI `_S5` (Soft Off) sleep state via PM1a / PM1b control blocks
//!   - QEMU / Bochs ACPI power-off ports (`0x604`, `0xB004`)
//!   - VirtualBox ACPI shutoff (`0x4004`)
//!   - 8042 PS/2 keyboard controller reset pulse (`0xFE`)
//!   - CPU Triple-Fault emergency hardware reset fallback

#![allow(dead_code)]

use crate::arch::io::{inb, outb, outw};
use crate::arch::acpi::ACPI_DATA;

/// Shuts down the machine cleanly using ACPI or hardware hypervisor ports.
pub fn shutdown() -> ! {
    crate::serial_println!("[Power] Initiating system shutdown sequence...");
    crate::println!("System is shutting down...");

    // Step 1: Attempt official ACPI S5 (Soft Off) sleep state
    {
        let acpi = ACPI_DATA.lock();
        if acpi.is_initialized {
            // Enable ACPI mode via SMI command port if required
            if acpi.smi_cmd != 0 && acpi.acpi_enable != 0 {
                unsafe {
                    outb(acpi.smi_cmd as u16, acpi.acpi_enable);
                    for _ in 0..10_000 { core::hint::spin_loop(); }
                }
            }

            // Write S5 sleep command to PM1a control block
            if acpi.pm1a_cnt_blk != 0 {
                let slp_en = 1u16 << 13; // Bit 13: Sleep Enable (SLP_EN)
                let val_a = ((acpi.slp_typa & 0x7) << 10) | slp_en;
                unsafe {
                    outw(acpi.pm1a_cnt_blk as u16, val_a);
                }

                if acpi.pm1b_cnt_blk != 0 {
                    let val_b = ((acpi.slp_typb & 0x7) << 10) | slp_en;
                    unsafe {
                        outw(acpi.pm1b_cnt_blk as u16, val_b);
                    }
                }

                for _ in 0..100_000 { core::hint::spin_loop(); }
            }
        }
    }

    // Step 2: QEMU ACPI power-off port (QEMU 0.14+)
    unsafe {
        outw(0x604, 0x2000);
        for _ in 0..10_000 { core::hint::spin_loop(); }
    }

    // Step 3: VirtualBox ACPI shutoff port
    unsafe {
        outw(0x4004, 0x3400);
        for _ in 0..10_000 { core::hint::spin_loop(); }
    }

    // Step 4: Older QEMU / Bochs shutoff port
    unsafe {
        outw(0xB004, 0x2000);
        for _ in 0..10_000 { core::hint::spin_loop(); }
    }

    // Step 5: Final fallback: disable interrupts and halt CPU indefinitely
    crate::serial_println!("[Power] Shutdown ports did not power off hardware; halting CPU.");
    crate::println!("It is now safe to turn off your computer.");

    loop {
        unsafe {
            core::arch::asm!("cli; hlt", options(nomem, nostack));
        }
    }
}

/// Reboots the computer via keyboard controller reset pulse or triple fault.
pub fn reboot() -> ! {
    crate::serial_println!("[Power] Initiating system reboot...");
    crate::println!("System is restarting...");

    // Step 1: Pulse 8042 PS/2 keyboard controller reset line (Output Port bit 0)
    unsafe {
        // Drain 8042 buffer
        for _ in 0..1000 {
            if (inb(0x64) & 2) == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        // Send Pulse Output Port command: bit 0 low (CPU RESET)
        outb(0x64, 0xFE);
        for _ in 0..100_000 { core::hint::spin_loop(); }
    }

    // Step 2: Emergency fallback — Trigger CPU Triple Fault
    // Loading an empty (size 0) IDTR and executing INT 3 forces an unrecoverable fault
    unsafe {
        #[repr(C, packed)]
        struct NullIdtr {
            limit: u16,
            base: u64,
        }
        let null_idtr = NullIdtr { limit: 0, base: 0 };
        core::arch::asm!(
            "lidt [{}]",
            "int 3",
            in(reg) &null_idtr,
            options(noreturn)
        );
    }
}
