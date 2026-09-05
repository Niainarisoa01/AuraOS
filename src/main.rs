#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

mod vga_buffer;
mod gdt;
mod idt;
mod io;
mod pic;
mod keyboard;
mod shell;

use core::panic::PanicInfo;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // ==========================================
    // Phase de démarrage du noyau AuraOS
    // ==========================================
    println!("============================================================");
    println!("        AuraOS Kernel v0.1.0 - Demarrage en cours...        ");
    println!("============================================================");
    println!();

    // Étape 1 : Initialiser la GDT (segments mémoire)
    gdt::init();
    println!("[OK] GDT       : Table Globale des Descripteurs chargee.");

    // Étape 2 : Initialiser et remapper le contrôleur PIC 8259
    pic::init();

    // Étape 3 : Initialiser l'IDT (table des interruptions et exceptions)
    idt::init();

    // Étape 4 : Activer les interruptions matérielles au niveau du CPU
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
    println!("[OK] CPU       : Interruptions materielles activees (sti).");

    // Étape 5 : Rapport système
    println!();
    println!("[OK] Mode      : x86_64 Bare-Metal Long Mode (64 bits)");
    println!("[OK] Affichage : VGA 80x25 avec scrolling automatique");
    println!("[OK] Clavier   : PS/2 actif (saisie en direct)");
    println!();
    println!("------------------------------------------------------------");
    println!("  AuraOS v0.1.0 pret. Console interactive active :");
    println!("------------------------------------------------------------");
    print!("auraos> ");

    // Boucle d'attente basse consommation (HLT) :
    // Le CPU se met en veille et ne se réveille que lorsqu'une
    // interruption matérielle survient (timer ou frappe clavier).
    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, preserves_flags)) };
    }
}

/// En cas de panique noyau, on affiche l'erreur à l'écran.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!();
    println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    println!("  KERNEL PANIC - AuraOS a rencontre une erreur fatale !");
    println!("  {}", info);
    println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, preserves_flags)) };
    }
}
