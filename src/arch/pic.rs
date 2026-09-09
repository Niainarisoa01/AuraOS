//! ============================================================================
//! 8259 PIC — Programmable Interrupt Controller (Dual 8259 PIC)
//! ============================================================================
//!
//! Standard x86 hardware uses two cascaded 8259 PIC chips to route
//! hardware interrupt lines (IRQs) to the CPU:
//!   - Master (PIC 1): Ports 0x20 (command) and 0x21 (data / mask)
//!   - Slave  (PIC 2): Ports 0xA0 (command) and 0xA1 (data / mask)
//!
//! At power-on, the BIOS maps IRQs 0-7 to CPU interrupt vectors 0x08-0x0F.
//! However, in 64-bit protected mode, these vectors are reserved for critical
//! CPU exceptions (e.g. Double Fault 0x08)!
//!
//! Therefore, we MUST remap the PIC so that hardware interrupts are shifted
//! beyond the CPU exceptions (vectors 32 to 47: 0x20 to 0x2F).

use super::io::{inb, io_wait, outb};

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

/// End of Interrupt (EOI) command byte
const PIC_EOI: u8 = 0x20;

/// Interrupt vector offset for Master PIC (IRQs 0..7 -> Vectors 32..39)
pub const PIC1_OFFSET: u8 = 32;
/// Interrupt vector offset for Slave PIC (IRQs 8..15 -> Vectors 40..47)
pub const PIC2_OFFSET: u8 = 40;

/// Specific hardware IRQs
pub const IRQ_TIMER: u8 = 0;
pub const IRQ_KEYBOARD: u8 = 1;
#[allow(dead_code)]
pub const IRQ_CASCADE: u8 = 2;
pub const IRQ_MOUSE: u8 = 12;

/// Initializes and remaps both 8259 PIC controllers.
pub fn init() {
    unsafe {
        // 1. Save current masks
        let _mask1 = inb(PIC1_DATA);
        let _mask2 = inb(PIC2_DATA);

        // 2. Start initialization sequence in cascade mode (ICW1 = 0x11)
        outb(PIC1_COMMAND, 0x11);
        io_wait();
        outb(PIC2_COMMAND, 0x11);
        io_wait();

        // 3. ICW2: Remap interrupt vectors
        outb(PIC1_DATA, PIC1_OFFSET); // Master: IRQ 0-7 -> 32-39 (0x20-0x27)
        io_wait();
        outb(PIC2_DATA, PIC2_OFFSET); // Slave:  IRQ 8-15 -> 40-47 (0x28-0x2F)
        io_wait();

        // 4. ICW3: Configure cascading between Master and Slave
        outb(PIC1_DATA, 0x04); // Tell Master that Slave is on IRQ2 (bit 2 = 4)
        io_wait();
        outb(PIC2_DATA, 0x02); // Tell Slave its cascade identity (2)
        io_wait();

        // 5. ICW4: 8086/88 mode
        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        // 6. Configure interrupt masks:
        //    Unmask IRQ 0 (Timer), IRQ 1 (Keyboard), and IRQ 2 (Cascade from Slave).
        //    0b1111_1000 = 0xF8
        outb(PIC1_DATA, 0xF8);
        //    Unmask IRQ 12 (PS/2 Mouse, line 4 on Slave PIC: 8 + 4 = 12).
        //    0b1110_1111 = 0xEF
        outb(PIC2_DATA, 0xEF);
    }

    crate::println!("[OK] PIC 8259  : Remapped successfully (IRQs 32..47).");
}

/// Dynamically unmasks a specific hardware IRQ line.
#[allow(dead_code)]
pub fn unmask_irq(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA) & !(1 << irq);
            outb(PIC1_DATA, mask);
        } else {
            let mask = inb(PIC2_DATA) & !(1 << (irq - 8));
            outb(PIC2_DATA, mask);
            // Ensure cascade line IRQ2 on Master is also unmasked
            let master_mask = inb(PIC1_DATA) & !(1 << 2);
            outb(PIC1_DATA, master_mask);
        }
    }
}

/// Signals the PIC that interrupt processing is complete (End Of Interrupt).
///
/// If the interrupt came from the Slave (IRQ >= 8), EOI must be sent
/// to BOTH the Slave and the Master. Otherwise, only to the Master.
pub fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_COMMAND, PIC_EOI);
        }
        outb(PIC1_COMMAND, PIC_EOI);
    }
}
