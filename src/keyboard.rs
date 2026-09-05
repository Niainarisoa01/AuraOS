/// ============================================================================
/// Pilote Clavier PS/2 (Scancode Set 1)
/// ============================================================================
///
/// Le contrôleur de clavier PS/2 (Intel 8042) génère une interruption IRQ 1
/// chaque fois qu'une touche est enfoncée ou relâchée.
///
/// Lorsqu'une touche est pressée : un « Make Code » est envoyé sur le port 0x60.
/// Lorsqu'une touche est relâchée: un « Break Code » (Make Code | 0x80) est envoyé.

use core::sync::atomic::{AtomicBool, Ordering};
use crate::io::inb;

const KEYBOARD_DATA_PORT: u16 = 0x60;

/// État de la touche Majuscule (Shift gauche ou droit)
static SHIFT_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Table de correspondance des scancodes PS/2 (Set 1) vers caractères ASCII (Minuscules)
static SCANCODE_LOWER: [u8; 58] = [
    0,    27,  b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', 8,   // 0x00 - 0x0E (8 = Backspace)
    b'\t', b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n',    // 0x0F - 0x1C
    0,    b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`',           // 0x1D - 0x29
    0,    b'\\', b'z', b'x', b'c', b'v', b'b', b'n', b'm', b',', b'.', b'/', 0,              // 0x2A - 0x35
    b'*', 0,   b' ',                                                                           // 0x36 - 0x39 (0x39 = Space)
];

/// Table de correspondance des scancodes PS/2 (Set 1) vers caractères ASCII (Majuscules)
static SCANCODE_UPPER: [u8; 58] = [
    0,    27,  b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_', b'+', 8,   // 0x00 - 0x0E
    b'\t', b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}', b'\n',    // 0x0F - 0x1C
    0,    b'A', b'S', b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':', b'"', b'~',           // 0x1D - 0x29
    0,    b'|', b'Z', b'X', b'C', b'V', b'B', b'N', b'M', b'<', b'>', b'?', 0,              // 0x2A - 0x35
    b'*', 0,   b' ',                                                                           // 0x36 - 0x39
];

/// Traite une frappe clavier lors d'une interruption IRQ 1.
pub fn handle_interrupt() {
    let scancode = unsafe { inb(KEYBOARD_DATA_PORT) };

    match scancode {
        // Shift gauche ou droit pressé
        0x2A | 0x36 => {
            SHIFT_ACTIVE.store(true, Ordering::Relaxed);
        }
        // Shift gauche ou droit relâché (Make Code | 0x80)
        0xAA | 0xB6 => {
            SHIFT_ACTIVE.store(false, Ordering::Relaxed);
        }
        // Si le bit 7 est à 1, c'est un relâchement de touche qu'on ignore
        code if code & 0x80 != 0 => {}
        // Touche enfoncée (Make Code valide)
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
                    // Backspace (Retour arrière)
                    8 => {
                        crate::shell::SHELL.lock().backspace();
                    }
                    // Entrée (Exécuter la commande)
                    b'\n' => {
                        crate::shell::SHELL.lock().enter();
                    }
                    // Caractère imprimable
                    0x20..=0x7E => {
                        crate::shell::SHELL.lock().push_char(ascii);
                    }
                    _ => {}
                }
            }
        }
    }
}
