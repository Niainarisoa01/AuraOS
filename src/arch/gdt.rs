/// ============================================================================
/// GDT — Global Descriptor Table
/// ============================================================================
///
/// The GDT defines memory segments for the x86_64 processor.
/// In 64-bit Long Mode, segmentation is mostly flattened, but remains mandatory
/// for CPU privilege separation (Ring 0 Kernel vs. Ring 3 User space).
///
/// Our minimal GDT contains 3 entries:
///   0. Null Descriptor (mandatory, can never be referenced)
///   1. Kernel Code Segment (Ring 0, executable)
///   2. Kernel Data Segment (Ring 0, read/write)

/// Representation of an 8-byte GDT entry.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GdtEntry {
    limit_low: u16,       // Segment limit (bits 0-15)
    base_low: u16,        // Base address (bits 0-15)
    base_middle: u8,      // Base address (bits 16-23)
    access: u8,           // Access byte: type, DPL, present bit
    granularity: u8,      // Flags + limit (bits 16-19)
    base_high: u8,        // Base address (bits 24-31)
}

impl GdtEntry {
    /// Creates a null GDT entry (all zeros).
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

    /// Creates a GDT entry with specified access and granularity flags.
    /// In 64-bit Long Mode, base and limit are ignored by the CPU
    /// (the segment spans the entire address space), but the access byte
    /// and the Long Mode flag (L-bit) are essential.
    pub const fn new(access: u8, flags: u8) -> Self {
        GdtEntry {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access,
            granularity: flags << 4 | 0x0F, // Flags in high nibble, limit in low nibble
            base_high: 0,
        }
    }
}

/// The GDTR pointer structure expected by the `lgdt` instruction.
#[repr(C, packed)]
pub struct GdtPointer {
    pub limit: u16,       // Size of the GDT table in bytes minus 1
    pub base: u64,        // Linear base address of the GDT table
}

// Access byte constants
const ACCESS_PRESENT: u8 = 0b1000_0000;  // Segment is present in memory
const ACCESS_RING0: u8 = 0b0000_0000;    // Privilege level 0 (Kernel)
const ACCESS_CODE_SEG: u8 = 0b0001_1010; // Executable, readable code segment
const ACCESS_DATA_SEG: u8 = 0b0001_0010; // Writable, readable data segment

// Granularity flags
const FLAG_LONG_MODE: u8 = 0b0010;       // 64-bit Long Mode flag (L-bit)

/// Static GDT table with 3 entries.
static GDT: [GdtEntry; 3] = [
    GdtEntry::null(),                                                                // 0x00: Null
    GdtEntry::new(ACCESS_PRESENT | ACCESS_RING0 | ACCESS_CODE_SEG, FLAG_LONG_MODE), // 0x08: Kernel Code Ring 0
    GdtEntry::new(ACCESS_PRESENT | ACCESS_RING0 | ACCESS_DATA_SEG, 0),               // 0x10: Kernel Data Ring 0
];

/// Loads the GDT into the processor's GDTR register and reloads segment selectors.
pub fn init() {
    let gdt_ptr = GdtPointer {
        limit: (core::mem::size_of_val(&GDT) - 1) as u16,
        base: GDT.as_ptr() as u64,
    };

    unsafe {
        // Load the GDT into the GDTR register via inline assembly
        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &gdt_ptr,
            options(readonly, nostack, preserves_flags)
        );

        // Reload segment registers with new selectors:
        //   CS (Code Segment)  = 0x08 (GDT index 1)
        //   DS, ES, SS, FS, GS = 0x10 (GDT index 2)
        core::arch::asm!(
            "push 0x08",           // Code segment selector (GDT index 1)
            "lea rax, [rip + 2f]", // Return address (label 2:)
            "push rax",
            "retfq",               // Far return to reload CS
            "2:",
            "mov ax, 0x10",        // Data segment selector (GDT index 2)
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            options(nostack)
        );
    }
}
