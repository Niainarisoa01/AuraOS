/// ============================================================================
/// GDT — Table Globale de Descripteurs (Global Descriptor Table)
/// ============================================================================
///
/// La GDT décrit les segments de mémoire au processeur x86_64.
/// En mode Long (64 bits), la segmentation est très simplifiée mais reste
/// obligatoire pour le passage Ring 0 (noyau) / Ring 3 (utilisateur).
///
/// Notre GDT minimale contient 3 entrées :
///   0. Le descripteur Null (obligatoire, ne peut jamais être référencé)
///   1. Le segment de code noyau (Ring 0, exécutable)
///   2. Le segment de données noyau (Ring 0, lecture/écriture)

/// Représentation d'une entrée GDT de 8 octets.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GdtEntry {
    limit_low: u16,       // Limite (bits 0-15)
    base_low: u16,        // Base (bits 0-15)
    base_middle: u8,      // Base (bits 16-23)
    access: u8,           // Octet d'accès : type, DPL, présence
    granularity: u8,      // Flags + limite (bits 16-19)
    base_high: u8,        // Base (bits 24-31)
}

impl GdtEntry {
    /// Crée une entrée GDT nulle (tout à zéro).
    pub const fn null() -> Self {
        GdtEntry {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }

    /// Crée une entrée GDT avec les paramètres spécifiés.
    /// En mode Long 64 bits, base et limite sont ignorées par le CPU
    /// (le segment couvre tout l'espace d'adressage), mais les flags d'accès
    /// et le bit Long Mode (L) restent essentiels.
    pub const fn new(access: u8, flags: u8) -> Self {
        GdtEntry {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access,
            granularity: flags << 4 | 0x0F, // flags dans les 4 bits hauts, limite haute = 0xF
            base_high: 0,
        }
    }
}

/// Le pointeur GDTR que l'instruction `lgdt` du processeur attend.
#[repr(C, packed)]
pub struct GdtPointer {
    pub limit: u16,       // Taille de la table GDT en octets - 1
    pub base: u64,        // Adresse de base de la table GDT en mémoire
}

// Constants pour les flags d'accès des segments
const ACCESS_PRESENT: u8 = 0b1000_0000;  // Le segment est en mémoire (Present)
const ACCESS_RING0: u8 = 0b0000_0000;    // Niveau de privilège 0 (Noyau)
const ACCESS_CODE_SEG: u8 = 0b0001_1010; // Segment de code exécutable, lisible
const ACCESS_DATA_SEG: u8 = 0b0001_0010; // Segment de données, inscriptible

// Flags de granularité
const FLAG_LONG_MODE: u8 = 0b0010;       // Bit L : mode Long 64 bits

/// Notre table GDT statique (3 entrées).
static GDT: [GdtEntry; 3] = [
    GdtEntry::null(),                                                              // 0x00 : Null
    GdtEntry::new(ACCESS_PRESENT | ACCESS_RING0 | ACCESS_CODE_SEG, FLAG_LONG_MODE), // 0x08 : Code Ring 0
    GdtEntry::new(ACCESS_PRESENT | ACCESS_RING0 | ACCESS_DATA_SEG, 0),             // 0x10 : Data Ring 0
];

/// Charge notre GDT dans le registre GDTR du processeur.
pub fn init() {
    let gdt_ptr = GdtPointer {
        limit: (core::mem::size_of_val(&GDT) - 1) as u16,
        base: GDT.as_ptr() as u64,
    };

    unsafe {
        // Charger la GDT dans le registre GDTR via l'instruction assembleur `lgdt`
        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &gdt_ptr,
            options(readonly, nostack, preserves_flags)
        );

        // Recharger les registres de segments avec les nouveaux sélecteurs :
        //   CS (Code Segment)  = 0x08 (index 1 de la GDT)
        //   DS, ES, SS, FS, GS = 0x10 (index 2 de la GDT)
        core::arch::asm!(
            "push 0x08",       // Sélecteur du segment de code (GDT index 1)
            "lea rax, [rip + 2f]", // Adresse de retour (label 2:)
            "push rax",
            "retfq",           // Far return pour recharger CS
            "2:",
            "mov ax, 0x10",    // Sélecteur du segment de données (GDT index 2)
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            options(nostack)
        );
    }
}
