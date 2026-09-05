/// ============================================================================
/// IDT — Table des Descripteurs d'Interruptions (Interrupt Descriptor Table)
/// ============================================================================
///
/// L'IDT est la table que le processeur consulte chaque fois qu'un événement
/// matériel ou logiciel survient (exception CPU, frappe clavier, timer, etc.).
///
/// Chaque entrée de l'IDT pointe vers une fonction Rust qui sera exécutée
/// automatiquement par le processeur lors de l'interruption correspondante.
///
/// En x86_64, chaque entrée mesure 16 octets et contient l'adresse de la
/// fonction de traitement (*handler*), le sélecteur de segment et les attributs.

/// Structure d'une entrée IDT en mode 64 bits (16 octets).
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IdtEntry {
    offset_low: u16,       // Adresse du handler (bits 0-15)
    selector: u16,         // Sélecteur de segment de code (0x08 = Ring 0)
    ist: u8,               // Index de la pile d'interruptions (IST), 0 = pas d'IST
    attributes: u8,        // Type, DPL, bit Present
    offset_mid: u16,       // Adresse du handler (bits 16-31)
    offset_high: u32,      // Adresse du handler (bits 32-63)
    reserved: u32,         // Réservé, doit être zéro
}

impl IdtEntry {
    /// Crée une entrée IDT vide (non configurée).
    pub const fn missing() -> Self {
        IdtEntry {
            offset_low: 0,
            selector: 0,
            ist: 0,
            attributes: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// Configure une entrée IDT pour pointer vers un handler donné.
    ///   - `handler` : Adresse de la fonction de traitement (ISR).
    ///   - `selector`: Sélecteur du segment de code (0x08 pour le code Ring 0).
    ///   - `attributes`: 0x8E = Interrupt Gate, Present, DPL 0.
    pub fn set_handler(&mut self, handler: u64) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = 0x08;   // Segment de code noyau (GDT index 1)
        self.ist = 0;
        self.attributes = 0x8E; // Interrupt Gate 64-bit, Present, DPL=0
        self.reserved = 0;
    }
}

/// Le pointeur IDTR que l'instruction `lidt` du processeur attend.
#[repr(C, packed)]
pub struct IdtPointer {
    pub limit: u16,
    pub base: u64,
}

/// Nombre maximal d'entrées IDT supportées par x86_64.
const IDT_ENTRIES: usize = 256;

use core::cell::UnsafeCell;

/// Wrapper thread-safe pour notre table IDT
struct IdtWrapper(UnsafeCell<[IdtEntry; IDT_ENTRIES]>);
unsafe impl Sync for IdtWrapper {}

/// Notre table IDT statique (256 entrées, toutes initialisées à « vide »).
static IDT: IdtWrapper = IdtWrapper(UnsafeCell::new([IdtEntry::missing(); IDT_ENTRIES]));

// ============================================================================
// Structures sauvegardées par le CPU lors d'une interruption
// ============================================================================

/// Le cadre d'interruption (*Interrupt Stack Frame*) est la structure que le
/// processeur empile automatiquement sur la pile du noyau avant d'appeler
/// notre handler. Elle contient l'état du code interrompu.
#[derive(Debug)]
#[repr(C)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,    // RIP : l'adresse à laquelle le code s'exécutait
    pub code_segment: u64,           // CS : le segment de code
    pub cpu_flags: u64,              // RFLAGS : les drapeaux du processeur
    pub stack_pointer: u64,          // RSP : le pointeur de pile
    pub stack_segment: u64,          // SS : le segment de pile
}

// ============================================================================
// Handlers (Gestionnaires) pour les exceptions CPU critiques
// ============================================================================

/// Exception 0 : Division par Zéro.
/// Se produit quand le code tente de diviser un nombre par 0.
extern "x86-interrupt" fn divide_by_zero_handler(frame: InterruptStackFrame) {
    crate::println!("\n[EXCEPTION] Division par zero !");
    crate::println!("  RIP (Instruction fautive) : {:#x}", frame.instruction_pointer);
    crate::println!("  Noyau arrete pour securite.");
    loop {}
}

/// Exception 6 : Opcode Invalide.
/// Se produit quand le CPU rencontre une instruction qu'il ne comprend pas.
extern "x86-interrupt" fn invalid_opcode_handler(frame: InterruptStackFrame) {
    crate::println!("\n[EXCEPTION] Opcode invalide (instruction inconnue) !");
    crate::println!("  RIP : {:#x}", frame.instruction_pointer);
    loop {}
}

/// Exception 8 : Double Faute.
/// C'est la pire des exceptions : elle se produit quand une exception survient
/// pendant le traitement d'une autre exception. Si elle n'est pas gérée,
/// le processeur effectue un Triple Fault (= reboot brutal).
extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, _error_code: u64) -> ! {
    crate::println!("\n[EXCEPTION CRITIQUE] DOUBLE FAULT !");
    crate::println!("  RIP : {:#x}", frame.instruction_pointer);
    crate::println!("  Le noyau ne peut pas continuer. Arret complet.");
    loop {}
}

/// Exception 13 : Faute de Protection Générale (GPF).
/// Se produit lors d'une violation de privilège ou d'un accès mémoire illégal.
extern "x86-interrupt" fn general_protection_fault_handler(frame: InterruptStackFrame, error_code: u64) {
    crate::println!("\n[EXCEPTION] General Protection Fault (GPF) !");
    crate::println!("  Code erreur : {:#x}", error_code);
    crate::println!("  RIP : {:#x}", frame.instruction_pointer);
    loop {}
}

/// Exception 14 : Faute de Page (Page Fault).
/// Se produit quand le code accède à une adresse mémoire virtuelle non mappée.
extern "x86-interrupt" fn page_fault_handler(frame: InterruptStackFrame, error_code: u64) {
    // Le registre CR2 contient l'adresse virtuelle fautive
    let cr2: u64;
    unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nostack, preserves_flags)) };

    crate::println!("\n[EXCEPTION] Page Fault (acces memoire invalide) !");
    crate::println!("  Adresse fautive (CR2) : {:#x}", cr2);
    crate::println!("  Code erreur : {:#x}", error_code);
    crate::println!("  RIP : {:#x}", frame.instruction_pointer);
    loop {}
}

// ============================================================================
// Handlers pour les Interruptions Matérielles (IRQs)
// ============================================================================

/// Compteur de « ticks » d'horloge système (déclenché environ 18.2 fois/sec par défaut).
static TICKS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// IRQ 0 : Horloge système (Timer PIT 8254).
extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    crate::pic::send_eoi(crate::pic::IRQ_TIMER);
}

/// IRQ 1 : Frappe Clavier PS/2.
extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    crate::keyboard::handle_interrupt();
    crate::pic::send_eoi(crate::pic::IRQ_KEYBOARD);
}

/// Retourne le nombre de ticks d'horloge écoulés depuis le démarrage.
#[allow(dead_code)]
pub fn ticks() -> u64 {
    TICKS.load(core::sync::atomic::Ordering::Relaxed)
}

// ============================================================================
// Initialisation de l'IDT
// ============================================================================

/// Enregistre tous les handlers dans la table IDT et la charge dans le CPU.
pub fn init() {
    let idt = IDT.0.get();
    unsafe {
        // Exceptions CPU (0..31)
        (*idt)[0].set_handler(divide_by_zero_handler as *const () as u64);
        (*idt)[6].set_handler(invalid_opcode_handler as *const () as u64);
        (*idt)[8].set_handler(double_fault_handler as *const () as u64);
        (*idt)[13].set_handler(general_protection_fault_handler as *const () as u64);
        (*idt)[14].set_handler(page_fault_handler as *const () as u64);

        // Interruptions Matérielles IRQs (32..47)
        (*idt)[crate::pic::PIC1_OFFSET as usize + crate::pic::IRQ_TIMER as usize]
            .set_handler(timer_handler as *const () as u64);
        (*idt)[crate::pic::PIC1_OFFSET as usize + crate::pic::IRQ_KEYBOARD as usize]
            .set_handler(keyboard_handler as *const () as u64);

        let idt_ptr = IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; IDT_ENTRIES]>() - 1) as u16,
            base: idt as u64,
        };

        core::arch::asm!(
            "lidt [{}]",
            in(reg) &idt_ptr,
            options(readonly, nostack, preserves_flags)
        );
    }

    crate::println!("[OK] IDT : 5 exceptions CPU + IRQ0 (Timer) + IRQ1 (Clavier) enregistrees.");
}
