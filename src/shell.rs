/// ============================================================================
/// Shell Interactif AuraOS — Terminal Bare-Metal
/// ============================================================================
///
/// Gère le buffer de saisie ligne par ligne et l'exécution des commandes
/// saisies au clavier par l'utilisateur.

use crate::vga_buffer::clear_screen;
use crate::io::outb;

const BUFFER_MAX: usize = 128;

pub struct Shell {
    buffer: [u8; BUFFER_MAX],
    length: usize,
}

impl Shell {
    pub const fn new() -> Self {
        Self {
            buffer: [0; BUFFER_MAX],
            length: 0,
        }
    }

    /// Ajoute un caractère tapé au clavier dans le buffer.
    pub fn push_char(&mut self, c: u8) {
        if self.length < BUFFER_MAX - 1 {
            self.buffer[self.length] = c;
            self.length += 1;
            crate::print!("{}", c as char);
        }
    }

    /// Efface le dernier caractère saisi.
    pub fn backspace(&mut self) {
        if self.length > 0 {
            self.length -= 1;
            crate::vga_buffer::backspace();
        }
    }

    /// Valide la ligne courante (touche Entrée) et exécute la commande.
    pub fn enter(&mut self) {
        crate::println!();

        if self.length > 0 {
            let len = self.length;
            self.length = 0;
            if let Ok(line) = core::str::from_utf8(&self.buffer[..len]) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    Self::execute(trimmed);
                }
            }
        }

        crate::print!("auraos> ");
    }

    /// Analyse et exécute la commande demandée.
    fn execute(cmd: &str) {
        let mut parts = cmd.split_whitespace();
        let command = parts.next().unwrap_or("");

        match command {
            "help" => {
                crate::println!("Commandes disponibles dans AuraOS v0.1.0 :");
                crate::println!("  help        - Affiche cette aide");
                crate::println!("  clear       - Efface l'ecran VGA");
                crate::println!("  info        - Affiche les informations systeme et CPU");
                crate::println!("  ticks       - Affiche le compteur de temps (PIT IRQ0)");
                crate::println!("  manifeste   - Vision et feuille de route 10 ans d'AuraOS");
                crate::println!("  calc <a+b>  - Calcule une addition simple");
                crate::println!("  reboot      - Redemarre la machine");
                crate::println!("  halt        - Met le processeur en veille prolongee");
            }

            "clear" => {
                clear_screen();
            }

            "info" => {
                crate::println!("============================================================");
                crate::println!("                    AURA OPERATING SYSTEM                   ");
                crate::println!("============================================================");
                crate::println!("  Version        : v0.1.0 (Bare-Metal Prototype)");
                crate::println!("  Architecture   : x86_64 Long Mode (64 bits pur Rust)");
                crate::println!("  Pilote Video   : VGA Text Mode 80x25 Buffer 0xb8000");
                crate::println!("  Controleur IRQ : Dual PIC 8259 remappe (32..47)");
                crate::println!("  Protection     : GDT 64-bit + IDT 256 entrees");
                crate::println!("  Clavier        : Pilote PS/2 (Set 1 Make/Break)");
                crate::println!("  Ticks Horloge  : {}", crate::idt::ticks());
                crate::println!("============================================================");
            }

            "ticks" => {
                crate::println!("Horloge systeme (IRQ0) : {} ticks", crate::idt::ticks());
            }

            "manifeste" => {
                crate::println!("--- AURAOS : VISION ET OBJECTIFS SUR 10 ANS ---");
                crate::println!("1. 100% Rust bare-metal : securite memoire garantie sans GC.");
                crate::println!("2. Hyper-leger : noyau complet < 1 Mo.");
                crate::println!("3. Micro-noyau modulaire avec drivers isoles.");
                crate::println!("4. UI vectorielle fluide avec typographie moderne.");
                crate::println!("------------------------------------------------");
            }

            "calc" => {
                let expr = parts.next().unwrap_or("");
                if let Some(pos) = expr.find('+') {
                    let left_str = &expr[..pos];
                    let right_str = &expr[pos + 1..];
                    if let (Ok(a), Ok(b)) = (parse_u64(left_str), parse_u64(right_str)) {
                        crate::println!("  {} + {} = {}", a, b, a + b);
                    } else {
                        crate::println!("  Erreur : nombres invalides. Exemple: calc 15+27");
                    }
                } else {
                    crate::println!("  Usage : calc <nombre>+<nombre>  (Exemple: calc 123+456)");
                }
            }

            "reboot" => {
                crate::println!("Redemarrage d'AuraOS en cours...");
                unsafe {
                    // Impulsion de reset via le contrôleur clavier 8042 (port 0x64, commande 0xFE)
                    outb(0x64, 0xFE);
                }
            }

            "halt" => {
                crate::println!("AuraOS est arrete. Processeur en veille prolongee.");
                loop {
                    unsafe {
                        core::arch::asm!("cli; hlt", options(nomem, nostack, preserves_flags));
                    }
                }
            }

            _ => {
                crate::println!("Commande inconnue : '{}'. Tapez 'help' pour la liste.", cmd);
            }
        }
    }
}

/// Convertit une tranche de texte en u64 sans bibliothèque standard.
fn parse_u64(s: &str) -> Result<u64, ()> {
    let s = s.trim();
    if s.is_empty() {
        return Err(());
    }
    let mut acc: u64 = 0;
    for b in s.bytes() {
        if b.is_ascii_digit() {
            acc = acc.checked_mul(10).ok_or(())?;
            acc = acc.checked_add((b - b'0') as u64).ok_or(())?;
        } else {
            return Err(());
        }
    }
    Ok(acc)
}

/// Instance globale unique du Shell AuraOS protégée par Spinlock.
pub static SHELL: crate::vga_buffer::Spinlock<Shell> =
    crate::vga_buffer::Spinlock::new(Shell::new());
