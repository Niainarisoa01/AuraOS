//! ============================================================================
//! GDT — Global Descriptor Table with TSS & Ring 3 Segments
//! ============================================================================
//!
//! The GDT defines memory segments for the x86_64 processor.
//! In 64-bit Long Mode, segmentation is mostly flattened, but remains mandatory
//! for CPU privilege separation (Ring 0 Kernel vs. Ring 3 User space).
//!
//! Our GDT contains 7 logical entries (spans 7 u64 slots, with TSS taking slots 3-4):
//!   0. Null Descriptor (mandatory, can never be referenced)
//!   1. Kernel Code Segment (Ring 0, executable)         — selector 0x08
//!   2. Kernel Data Segment (Ring 0, read/write)         — selector 0x10
//!   3-4. TSS Descriptor (16 bytes, type 0x89)           — selector 0x18
//!   5. User Data Segment (Ring 3, read/write)           — selector 0x23
//!   6. User Code Segment (Ring 3, executable, 64-bit)   — selector 0x2B
//!
//! Note: User Data must come BEFORE User Code for sysret to work correctly.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

/// Safe wrapper for static mutable bare-metal structures
struct SyncUnsafeCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncUnsafeCell<T> {}

impl<T> SyncUnsafeCell<T> {
    const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }
    #[inline(always)]
    fn get(&self) -> *mut T {
        self.0.get()
    }
}

/// Indicates whether the TSS has been loaded (for test verification)
pub static TSS_LOADED: AtomicBool = AtomicBool::new(false);

// ============================================================================
// TSS — Task State Segment (64-bit)
// ============================================================================

/// The 64-bit TSS structure (104 bytes).
/// Used for stack switching on privilege level changes and IST entries.
#[repr(C, packed)]
pub struct Tss64 {
    reserved0: u32,
    /// Ring 0 stack pointer — used when transitioning from Ring 3 to Ring 0
    pub rsp0: u64,
    /// Ring 1 stack pointer (unused in our design)
    pub rsp1: u64,
    /// Ring 2 stack pointer (unused in our design)
    pub rsp2: u64,
    reserved1: u64,
    /// Interrupt Stack Table entries 1-7
    /// IST1 is used for Double Fault handler to prevent Triple Faults
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    reserved2: u64,
    reserved3: u16,
    /// I/O Map Base Address (set to size of TSS to disable I/O bitmap)
    pub iopb_offset: u16,
}

impl Tss64 {
    pub const fn new() -> Self {
        Tss64 {
            reserved0: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist1: 0,
            ist2: 0,
            ist3: 0,
            ist4: 0,
            ist5: 0,
            ist6: 0,
            ist7: 0,
            reserved2: 0,
            reserved3: 0,
            iopb_offset: 104, // sizeof(Tss64)
        }
    }
}

#[repr(align(16))]
struct AlignedStack<const N: usize>([u8; N]);

/// IST1 stack for Double Fault handler (8 KiB, 16-byte aligned)
const IST1_STACK_SIZE: usize = 8 * 1024;
static IST1_STACK: SyncUnsafeCell<AlignedStack<IST1_STACK_SIZE>> = SyncUnsafeCell::new(AlignedStack([0u8; IST1_STACK_SIZE]));

/// Kernel Ring 0 stack for transitions from Ring 3 (16 KiB, 16-byte aligned)
const KERNEL_STACK_SIZE: usize = 16 * 1024;
static KERNEL_STACK: SyncUnsafeCell<AlignedStack<KERNEL_STACK_SIZE>> = SyncUnsafeCell::new(AlignedStack([0u8; KERNEL_STACK_SIZE]));

/// Global TSS instance
static TSS: SyncUnsafeCell<Tss64> = SyncUnsafeCell::new(Tss64::new());

// ============================================================================
// GDT Entry Structures
// ============================================================================

/// Representation of an 8-byte GDT entry.
#[allow(dead_code)]
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

#[allow(dead_code)]
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
    pub const fn new(access: u8, flags: u8) -> Self {
        GdtEntry {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access,
            granularity: flags << 4 | 0x0F,
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

// Access byte constants (with Accessed bit 0 set to 1 so CPU does not write to GDT)
const ACCESS_PRESENT: u8 = 0b1000_0000;  // Segment is present in memory
const ACCESS_RING0: u8 = 0b0000_0000;    // Privilege level 0 (Kernel)
const ACCESS_RING3: u8 = 0b0110_0000;    // Privilege level 3 (User)
const ACCESS_CODE_SEG: u8 = 0b0001_1011; // Executable, readable code segment + Accessed bit
const ACCESS_DATA_SEG: u8 = 0b0001_0011; // Writable, readable data segment + Accessed bit

// Granularity flags
const FLAG_LONG_MODE: u8 = 0b0010;       // 64-bit Long Mode flag (L-bit)

/// Static GDT table with 7 raw u64 slots:
///   [0]   = Null
///   [1]   = Kernel Code 64-bit  (selector 0x08)
///   [2]   = Kernel Data          (selector 0x10)
///   [3-4] = TSS Descriptor (16 bytes, selector 0x18)
///   [5]   = User Data            (selector 0x23, DPL=3)
///   [6]   = User Code 64-bit    (selector 0x2B, DPL=3)
static GDT: SyncUnsafeCell<[u64; 7]> = SyncUnsafeCell::new([0u64; 7]);

/// Kernel code segment selector
#[allow(dead_code)]
pub const KERNEL_CS: u16 = 0x08;
/// Kernel data segment selector
#[allow(dead_code)]
pub const KERNEL_DS: u16 = 0x10;
/// TSS segment selector
pub const TSS_SEL: u16 = 0x18;
/// User data segment selector (Slot 5: 0x28, DPL=3, RPL=3)
#[allow(dead_code)]
pub const USER_DS: u16 = 0x28 | 3; // 0x2B
/// User code segment selector (Slot 6: 0x30, DPL=3, RPL=3)
#[allow(dead_code)]
pub const USER_CS: u16 = 0x30 | 3; // 0x33

/// Encodes a standard 8-byte GDT descriptor into a u64
const fn encode_gdt_entry(access: u8, flags: u8) -> u64 {
    let limit_low: u64 = 0xFFFF;
    let granularity: u64 = ((flags << 4) | 0x0F) as u64;
    limit_low
        | (0u64 << 16)              // base_low
        | (0u64 << 32)              // base_middle
        | ((access as u64) << 40)   // access
        | (granularity << 48)       // granularity
        | (0u64 << 56)              // base_high
}

/// Encodes the TSS descriptor into two u64 words (16 bytes total).
fn encode_tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let base_low = base & 0xFFFF;
    let base_mid = (base >> 16) & 0xFF;
    let base_mid_high = (base >> 24) & 0xFF;
    let base_high = base >> 32;
    let limit_low = (limit & 0xFFFF) as u64;
    let limit_high = ((limit >> 16) & 0xF) as u64;

    // Access byte: Present=1, DPL=0, Type=0x9 (Available 64-bit TSS)
    let access: u64 = 0x89;

    let low = limit_low
        | (base_low << 16)
        | (base_mid << 32)
        | (access << 40)
        | (limit_high << 48)
        | (base_mid_high << 56);

    let high = base_high;

    (low, high)
}

/// Loads the GDT into the processor's GDTR register, configures the TSS,
/// and reloads segment selectors.
pub fn init() {
    unsafe {
        // Configure TSS
        let ist1_top = (IST1_STACK.get() as u64 + IST1_STACK_SIZE as u64) & !0xFu64;
        let kernel_stack_top = (KERNEL_STACK.get() as u64 + KERNEL_STACK_SIZE as u64) & !0xFu64;

        let tss = TSS.get();
        (*tss).ist1 = ist1_top;        // IST1 for Double Fault
        (*tss).rsp0 = kernel_stack_top; // Ring 0 stack for Ring 3 → Ring 0 transitions

        // Build GDT entries
        let gdt = GDT.get();
        (*gdt)[0] = 0; // Null descriptor
        (*gdt)[1] = encode_gdt_entry(ACCESS_PRESENT | ACCESS_RING0 | ACCESS_CODE_SEG, FLAG_LONG_MODE); // Kernel Code
        (*gdt)[2] = encode_gdt_entry(ACCESS_PRESENT | ACCESS_RING0 | ACCESS_DATA_SEG, 0); // Kernel Data

        // TSS Descriptor (spans slots 3 and 4)
        let tss_base = tss as u64;
        let tss_limit = (core::mem::size_of::<Tss64>() - 1) as u32;
        let (tss_low, tss_high) = encode_tss_descriptor(tss_base, tss_limit);
        (*gdt)[3] = tss_low;
        (*gdt)[4] = tss_high;

        // User Data (selector 0x20, with RPL=3 → 0x23)
        (*gdt)[5] = encode_gdt_entry(ACCESS_PRESENT | ACCESS_RING3 | ACCESS_DATA_SEG, 0);
        // User Code 64-bit (selector 0x28, with RPL=3 → 0x2B)
        (*gdt)[6] = encode_gdt_entry(ACCESS_PRESENT | ACCESS_RING3 | ACCESS_CODE_SEG, FLAG_LONG_MODE);

        // Load GDT
        let gdt_ptr = GdtPointer {
            limit: (core::mem::size_of::<[u64; 7]>() - 1) as u16,
            base: gdt as u64,
        };

        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &gdt_ptr,
            options(readonly, nostack, preserves_flags)
        );

        // Reload segment registers with kernel selectors
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
            out("rax") _,
        );

        // Load TSS selector into the Task Register
        core::arch::asm!(
            "ltr {0:x}",
            in(reg) TSS_SEL as u64,
            options(nostack, preserves_flags)
        );

        TSS_LOADED.store(true, Ordering::SeqCst);
    }

    crate::println!("[OK] GDT       : Global Descriptor Table loaded (7 entries + TSS).");
    crate::serial_println!("[OK] GDT loaded with TSS (IST1={} KiB, RSP0={} KiB).",
        IST1_STACK_SIZE / 1024, KERNEL_STACK_SIZE / 1024);
}

/// Returns the current TSS RSP0 value (for verification in tests)
#[allow(dead_code)]
pub fn tss_rsp0() -> u64 {
    unsafe { (*TSS.get()).rsp0 }
}

/// Sets the TSS RSP0 value (kernel stack pointer used on Ring 3 -> Ring 0 transition)
pub fn set_tss_rsp0(rsp0: u64) {
    unsafe {
        (*TSS.get()).rsp0 = rsp0;
    }
}

/// Returns the current TSS IST1 value (for verification in tests)
#[allow(dead_code)]
pub fn tss_ist1() -> u64 {
    unsafe { (*TSS.get()).ist1 }
}
