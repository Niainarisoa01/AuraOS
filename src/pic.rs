/// ============================================================================
/// PIC 8259 — Contrôleur Programmable d'Interruptions (Dual 8259 PIC)
/// ============================================================================
///
/// Le PC x86 standard utilise deux puces 8259 en cascade pour acheminer
/// les interruptions matérielles (IRQs) vers le processeur :
///   - Maître (PIC 1) : Ports 0x20 (commande) et 0x21 (données / masque)
///   - Esclave (PIC 2) : Ports 0xA0 (commande) et 0xA1 (données / masque)
///
/// Par défaut à l'allumage, le BIOS configure le PIC pour envoyer les IRQ 0-7
/// sur les vecteurs d'interruption 0x08-0x0F. OR, en mode 64 bits protégé,
/// ces vecteurs sont réservés aux exceptions critiques du CPU (ex: Double Fault 0x08) !
///
/// Il est donc IMPÉRATIF de « remapper » le PIC pour décaler les interruptions
/// matérielles au-delà des exceptions CPU (vecteurs 32 à 47 : 0x20 à 0x2F).

use crate::io::{inb, io_wait, outb};

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

/// Commande de fin d'interruption (End of Interrupt - EOI)
const PIC_EOI: u8 = 0x20;

/// Décalage d'interruption pour le PIC Maître (IRQs 0..7 -> Vecteurs 32..39)
pub const PIC1_OFFSET: u8 = 32;
/// Décalage d'interruption pour le PIC Esclave (IRQs 8..15 -> Vecteurs 40..47)
pub const PIC2_OFFSET: u8 = 40;

/// IRQ spécifiques
pub const IRQ_TIMER: u8 = 0;
pub const IRQ_KEYBOARD: u8 = 1;

/// Initialise et remappe les deux contrôleurs PIC 8259.
pub fn init() {
    unsafe {
        // 1. Sauvegarder les masques actuels
        let _mask1 = inb(PIC1_DATA);
        let _mask2 = inb(PIC2_DATA);

        // 2. Début de l'initialisation en mode cascade (ICW1 = 0x11)
        outb(PIC1_COMMAND, 0x11);
        io_wait();
        outb(PIC2_COMMAND, 0x11);
        io_wait();

        // 3. ICW2 : Décalage des vecteurs d'interruptions
        outb(PIC1_DATA, PIC1_OFFSET); // Master : IRQ 0-7 -> 32-39 (0x20-0x27)
        io_wait();
        outb(PIC2_DATA, PIC2_OFFSET); // Slave  : IRQ 8-15 -> 40-47 (0x28-0x2F)
        io_wait();

        // 4. ICW3 : Configuration de la cascade entre Maître et Esclave
        outb(PIC1_DATA, 0x04); // Dire au Maître que l'Esclave est connecté sur IRQ 2 (bit 2 = 4)
        io_wait();
        outb(PIC2_DATA, 0x02); // Dire à l'Esclave son identité de cascade (2)
        io_wait();

        // 5. ICW4 : Mode 8086/88
        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        // 6. Configurer les masques d'interruptions :
        //    Activer IRQ 0 (Timer) et IRQ 1 (Clavier).
        //    Désactiver (masquer) toutes les autres IRQs pour éviter les bruits parasites.
        //    Bit à 0 = actif, bit à 1 = masqué.
        //    0b1111_1100 = 0xFC (IRQ0 et IRQ1 démasqués)
        outb(PIC1_DATA, 0xFC);
        outb(PIC2_DATA, 0xFF); // Toutes les IRQs esclaves masquées
    }

    crate::println!("[OK] PIC 8259 : Remappage termine (IRQs 32..47).");
}

/// Signale au PIC que le traitement d'une interruption est terminé (End Of Interrupt).
///
/// Si l'interruption provient de l'Esclave (IRQ >= 8), il faut envoyer l'EOI
/// à la fois à l'Esclave ET au Maître. Sinon, seulement au Maître.
pub fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_COMMAND, PIC_EOI);
        }
        outb(PIC1_COMMAND, PIC_EOI);
    }
}
