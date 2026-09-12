//! ============================================================================
//! Architectural Support for Multi-Processor (SMP) Systems
//! ============================================================================
//!
//! This module provides the low-level building blocks that are shared between
//! the Bootstrap Processor (BSP) and every Application Processor (AP):
//!
//!   - Per-CPU GDT / TSS / IST1 stacks / kernel stacks (per-CPU data segment
//!     tables with the same selectors, but a distinct per-CPU TSS each).
//!   - The CPU-local LAPIC timer (vector 0x31) used for the per-CPU preemptive
//!     scheduler tick.
//!   - The INIT-SIPI-SIPI wake-up trampoline that brings each AP from real mode
//!     to long mode with its own GDT/TSS/IST/stacks, then hands it to the SMP
//!     per-CPU scheduler.
//!
//! NOTE: `PerCpu` is the authoritative per-CPU record. The first 0x28 bytes of
//! every `PerCpu` are the GDT/TSS/stack *state* used by the CPU's own segment
//! setup; `gdt::build_per_cpu()` fills them via the CPU-local GDT table.

use crate::arch::gdt::{GdtEntry, Tss64, AlignedStack, IST1_STACK_SIZE, KERNEL_STACK_SIZE};
use crate::arch::gdt::{encode_gdt_entry, encode_tss_descriptor};
use crate::arch::msr::{rdmsr, wrmsr};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::sync::Spinlock;

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of logical CPUs the kernel supports (MADT ceiling).
pub const MAX_CPUS: usize = 8;

/// Trampoline physical address (low memory, identity mapped by PMM).
pub const TRAMPOLINE_BASE: u64 = 0x8000;

/// Physical address where the AP bootstrap scratch lives.
pub const AP_SCRATCH_BASE: u64 = 0x9000;

/// Per-CPU data blocked in low-memory shared region.
pub const SHARED_SCRATCH_SIZE: usize = 0x1000;

/// Per-CPU spinlock for the (single) multi-CPU scheduler.
static SCHED_LOCK: Spinlock<()> = Spinlock::new(());

// ============================================================================
// Per-CPU Data
// ============================================================================

/// A 16-byte aligned per-CPU stack block.
#[repr(align(16))]
pub struct AlignedStack<const N: usize>([u8; N]);

/// Per-CPU kernel ring 0 stack size (16 KiB).
pub(crate) const KERNEL_STACK_SIZE: usize = 16 * 1024;

/// Per-CPU IST1 (Double Fault) stack size (8 KiB).
pub(crate) const IST1_STACK_SIZE: usize = 8 * 1024;

/// Per-CPU stack blocks (owned by `PerCpu`).
struct CpuStacks {
    kernel_stack: AlignedStack<KERNEL_STACK_SIZE>,
    ist1_stack:   AlignedStack<IST1_STACK_SIZE>,
}

/// One complete per-CPU segment-state block: the GDT entries (16 bytes per
/// entry, but we keep one u64 per GDT slot + 2 for the TSS descriptor
/// spanning slots 3-4, mirroring `gdt::init()`), plus the TSS the descriptor
/// points at, plus the two per-CPU stacks.
///
/// Layout (repr(C, align(16))), offsets relative to the start of the block —
/// these are exactly the offsets the naked syscall entry assembly expects for
/// the user-RSP / kernel-RSP / syscall-number scratch slots (gs:0x00 / 0x08 /
/// 0x10), followed by CPU identity fields.
#[repr(C, align(16))]
pub struct PerCpu {
    // ---- Offsets 0x00/0x08/0x10: syscall per-CPU scratch (GS) ----
    pub user_rsp_scratch:   u64,   // gs:0x00
    pub kernel_rsp_scratch: u64,   // gs:0x08
    pub syscall_nr_scratch: u64,   // gs:0x10
    // ---- Offsets 0x18/0x20: CPU identity ----
    pub cpu_id:             u64,   // gs:0x18
    pub apic_id:            u64,   // gs:0x20
    // ---- Segment state (GDT/TSS/stacks) ----
    pub gdt:                [u64; 10],   // 10 u64 = 5 slots; TSS descriptor spans 2
    pub tss:                Tss64,
    pub kernel_stack:       AlignedStack<KERNEL_STACK_SIZE>,
    pub ist1_stack:         AlignedStack<IST1_STACK_SIZE>,
    /// Set once this CPU has finished its AP bootstrap.
    pub online:             AtomicBool,
}

/// The per-CPU blocks themselves, one for each possible CPU id.
pub static PER_CPU: SyncUnsafeCell<[PerCpu; MAX_CPUS]> =
    SyncUnsafeCell::new([PerCpu::new(0); MAX_CPUS]);

// ============================================================================
// Per-CPU construction & segment installation
// ============================================================================

impl PerCpu {
    pub const fn new(cpu_id: u64) -> Self {
        PerCpu {
            user_rsp_scratch: 0,
            kernel_rsp_scratch: 0,
            syscall_nr_scratch: 0,
            cpu_id,
            apic_id: 0,
            gdt: [0u64; 10],
            tss: Tss64::new(),
            kernel_stack: AlignedStack([0u8; KERNEL_STACK_SIZE]),
            ist1_stack: AlignedStack([0u8; IST1_STACK_SIZE]),
            online: AtomicBool::new(false),
        }
    }
}

/// Installs the per-CPU GDT/TSS/IST1/stacks for the given CPU id on the
/// CURRENT processor. The `lgdt`/`ltr` sequence and stack pointers come from
/// `p` (this CPU's `PerCpu` block). SCATTERWRITEs the syscall MSRs after.
///
/// # Safety
/// The caller must ensure the `PerCpu` block is zeroed and that the trampoline
/// region (identity-mapped low physical memory) is writable BEFORE this runs.
pub(crate) fn init_cpu_state(p: *mut PerCpu) {
    unsafe {
        // Build the 7-slot GDT (same layout as gdt::init), but TSS base = this
        // CPU's own TSS, and IST1/RSP0 = this CPU's own stacks.
        let tss = &mut (*p).tss;
        tss.ist1 = (*p).ist1_stack.as_ptr() as u64 + IST1_STACK_SIZE as u64;
        (*tss).rsp0 = (*p).kernel_stack.as_ptr() as u64 + KERNEL_STACK_SIZE as u64;

        let gdt = &mut (*p).gdt;
        (*gdt)[0] = 0; // Null
        (*gdt)[1] = encode_gdt_entry(0b1001_1011, 0b0010_0000); // Kernel Code (0x08)
        (*gdt)[2] = encode_gdt_entry(0b1001_0011, 0);           // Kernel Data (0x10)
        // TSS descriptor spans slots 3-4 (selector 0x18)
        let (lo, hi) = encode_tss_descriptor(tss as u64, core::mem::size_of::<Tss64>() as u32 - 1);
        (*gdt)[3] = lo;
        (*gdt)[4] = hi;
        (*gdt)[5] = encode_gdt_entry(0b1111_0011, 0);           // User Data (0x28)
        (*gdt)[6] = encode_gdt_entry(0b1111_1011, 0b0010_0000); // User Code (0x30)

        // Load this CPU's GDT and TSS
        let gdt_ptr = crate::arch::gdt::GdtPointer {
            limit: (core::mem::size_of::<[u64; 10]>() - 1) as u16,
            base: gdt as u64,
        };
        core::arch::asm!("lgdt [{}]", in(reg) &gdt_ptr, options(readonly, nostack, preserves_flags));
        core::arch::asm!("ltr {0:x}", in(reg) 0x18u64, options(nostack, preserves_flags));
    }
}

/// Writes the syscall MSRs for THIS CPU (must run on the CPU it configures).
pub fn init_cpu_syscall_msrs() {
    crate::arch::syscall::init_on_cpu();
}

/// Returns the number of online CPUs (BSP + APs).
pub fn cpu_count() -> usize {
    let mut n = 0;
    for i in 0..MAX_CPUS {
        // All CPUs start online=false; only init sets true. For safety during
        // partial bring-up, cpu 0 (BSP) counts even before smp::init runs.
        if i == 0 || PER_CPU.get_mut()[i].is_online() {
            n += 1;
        }
    }
    n
}

/// Returns the local CPU id (GS base → cpu_id field).
pub fn current_cpu() -> usize {
    let id: u64;
    unsafe {
        core::arch::asm!("mov {}, gs:[0x18]", out(reg) id, options(nomem, nostack, preserves_flags));
    }
    id as usize
}

impl PerCpu {
    fn is_online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }
    fn set_online(&self) {
        self.online.store(true, Ordering::SeqCst);
    }
}

// ============================================================================
// AP trampoline (INIT-SIPI-SIPI)
// ============================================================================

// NOTE: the real trampoline lives in `#[naked]`/`global_asm!` below; the wakeup
// is driven by `smp.rs` which is wired in `arch/mod.rs` + `main.rs` by the SMP
// chantier. This file holds the trampoline entry stub that each AP executes.
//
// The stub is copied to `TRAMPOLINE_BASE` (0x8000) in real mode; APs enter at
// the physical address recorded in the SIPI vector. It:
//   1. disables interrupts,
//   2. reprograms stacks/GDT from the identity-mapped low-memory scratch,
//   3. enables PAE + long mode, jumps to `ap_start64` in 64-bit mode.

core::arch::global_asm!(
    ".global {tramp_start}",
    ".global {tramp_end}",
    ".code16",
    "{tramp_start}:",
    "   cli",
    "   cld",
    "   xor ax, ax",
    "   mov ds, ax",
    "   mov es, ax",
    "   mov ss, ax",
    "   xor sp, sp",
    "   mov sp, 0x8C00",              // provisional stack (grows down from 0x8C00)
    // Read scratch: cpuid is at 0x9000+0x18... but GS needs the block address:
    "   mov edx, 0x9000",             // scratch base (identity mapped low)
    // ap_start64 vaddr stored at 0x9000+0x10 by the BSP:
    "   lgdt [tramp_gdt_ptr]",        // GDT in this file (identity low)
    "   mov eax, cr0",
    "   or eax, 1",                   // PE
    "   mov cr0, eax",
    "   ljmp 0x08:pm32",
    ".code32",
    "pm32:",
    "   mov eax, cr4",
    "   or eax, 1 << 5",              // PAE
    "   mov cr4, eax",
    "   mov ecx, 0xC0000080",         // EFER
    "   rdmsr",
    "   or eax, 1 << 8",              // LME
    "   wrmsr",
    "   mov eax, [0x9000]",           // kernel CR3 (physical) from scratch
    "   mov cr3, eax",
    "   mov eax, cr0",
    "   or eax, 0x80000000",          // PG
    "   mov cr0, eax",
    "   ljmp 0x08:lm64",
    ".code64",
    "lm64:",
    "   mov rsp, [0x9000+0x08]",      // this CPU's temporary stack top
    "   mov rax, [0x9000+0x10]",      // ap_start64 vaddr
    "   call rax",
    "{tramp_end}:",
    ".align 16",
    "tramp_gdt_ptr: .word tramp_gdt_end - tramp_gdt - 1",
    "                .long tramp_gdt",
    "tramp_gdt: .quad 0, 0x00AF9A000000FFFF, 0x00CF92000000FFFF",
    "tramp_gdt_end:",
);

/// Copies the trampoline stub into the identity-mapped low-memory region.
pub fn copy_trampoline() {
    extern "C" {
        static tramp_start: u8;
        static tramp_end: u8;
    }
    let start = unsafe { &tramp_start as *const u8 as u64 };
    let end = unsafe { &tramp_end as *const u8 as u64 };
    let len = (end - start) as usize;
    if len > 0x1000 {
        panic!("trampoline too large");
    }
    unsafe {
        core::ptr::copy_nonoverlapping(start as *const u8, TRAMPOLINE_BASE as *mut u8, len);
    }
    crate::serial_println!("[SMP] Trampoline: {} bytes @ {:#x}", len, TRAMPOLINE_BASE);
}
