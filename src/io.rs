/// ============================================================================
/// Communications avec les Ports d'Entrée/Sortie (I/O Ports x86)
/// ============================================================================
///
/// L'architecture x86 dispose d'un espace d'adressage I/O distinct de la mémoire RAM.
/// Pour communiquer avec les périphériques matériels de base (PIC, Clavier PS/2,
/// Port Série, Horloge RTC, etc.), le processeur utilise les instructions assembleur
/// `in` et `out`.

/// Écrit un octet (8 bits) sur un port I/O donné.
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

/// Lit un octet (8 bits) depuis un port I/O donné.
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

/// Attente I/O minimale (~1 à 4 microsecondes).
///
/// Écrire sur le port 0x80 (traditionnellement réservé aux cartes de diagnostic POST)
/// est le standard pour réaliser ce délai sans effet secondaire.
#[inline]
pub unsafe fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}
