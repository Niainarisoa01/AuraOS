//! ============================================================================
//! Architectural Support for Multi-Processor (SMP) Systems
//! ============================================================================
//!
//! This module provides the low-level building blocks that are shared between
//! the Bootstrap Processor (BSP) and every Application Processor (AP):
//!
//!   - Per-CPU GDT / TSS / IST1 stacks / kernel stacks (per-CPU data segment
//!     tables with the same selectors, but a distinct per-CPU TSS each).
//!   - The INIT-SIPI-SIPI wake-up sequence that brings each AP from real mode
//!     to long mode with its own GDT/TSS/IST/stacks, then parks it in a HLT
//!     idle loop until the per-CPU scheduler is implemented (phase 2).
//!
//! NOTE: `PerCpu` is the authoritative per-CPU record.  The syscall scratch
//! slots at GS:0x00/0x08/0x10 are filled by `init_cpu_state` and used by the
//! naked syscall entry assembly.

use crate::arch::gdt::{Tss64, AlignedStack, IST1_STACK_SIZE, KERNEL_STACK_SIZE};
use crate::arch::gdt::{encode_gdt_entry, encode_tss_descriptor, SyncUnsafeCell};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of logical CPUs the kernel supports (MADT ceiling).
pub const MAX_CPUS: usize = 8;

/// Trampoline physical address (low memory, identity mapped by PMM).
pub const TRAMPOLINE_BASE: u64 = 0x8000;

/// Physical address where the AP bootstrap scratch lives.
pub const AP_SCRATCH_BASE: u64 = 0x9000;

// ============================================================================
// Per-CPU Data
// ============================================================================

/// One complete per-CPU segment-state block: the GDT entries, the TSS the
/// descriptor points at, plus two per-CPU stacks.
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
pub static PER_CPU: SyncUnsafeCell<[PerCpu; MAX_CPUS]> = SyncUnsafeCell::new(
    [const { PerCpu::new_const() }; MAX_CPUS]
);

/// Number of online CPUs (BSP sets to 1, each AP increments atomically).
pub static ONLINE_CPUS: AtomicU32 = AtomicU32::new(0);

/// Global synchronization flag: BSP signals APs to begin timer preemption and scheduling.
pub static SMP_STARTED: AtomicBool = AtomicBool::new(false);

/// Called by BSP when the kernel is ready for APs to start their timers and scheduler loops.
pub fn start_aps() {
    SMP_STARTED.store(true, Ordering::Release);
}

/// Returns true if the BSP has released the APs to start execution.
#[allow(dead_code)]
pub fn is_smp_started() -> bool {
    SMP_STARTED.load(Ordering::Acquire)
}

// ============================================================================
// Per-CPU construction & segment installation
// ============================================================================

impl PerCpu {
    /// Const-initializer for the per-CPU block (all zeroed).
    const fn new_const() -> Self {
        PerCpu {
            user_rsp_scratch: 0,
            kernel_rsp_scratch: 0,
            syscall_nr_scratch: 0,
            cpu_id: 0,
            apic_id: 0,
            gdt: [0u64; 10],
            tss: Tss64::new(),
            kernel_stack: AlignedStack::new(),
            ist1_stack: AlignedStack::new(),
            online: AtomicBool::new(false),
        }
    }

    fn is_online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    fn set_online(&self) {
        self.online.store(true, Ordering::SeqCst);
    }
}

/// Installs the per-CPU GDT/TSS/IST1/stacks for the given CPU id on the
/// CURRENT processor. The `lgdt`/`ltr` sequence and stack pointers come from
/// `p` (this CPU's `PerCpu` block).
///
/// # Safety
/// The caller must ensure the `PerCpu` block is initialized and that the
/// trampoline region (identity-mapped low physical memory) is writable.
pub(crate) unsafe fn init_cpu_state(p: *mut PerCpu) {
    unsafe {
        // Build the 7-slot GDT (same layout as gdt::init), but TSS base = this
        // CPU's own TSS, and IST1/RSP0 = this CPU's own stacks.
        let tss = &mut (*p).tss;
        tss.ist1 = (*p).ist1_stack.as_ptr() as u64 + IST1_STACK_SIZE as u64;
        tss.rsp0 = (*p).kernel_stack.as_ptr() as u64 + KERNEL_STACK_SIZE as u64;

        let gdt = &mut (*p).gdt;
        gdt[0] = 0; // Null
        gdt[1] = encode_gdt_entry(0b1001_1011, 0b0010); // Kernel Code (0x08, L-bit=1)
        gdt[2] = encode_gdt_entry(0b1001_0011, 0);      // Kernel Data (0x10)
        // TSS descriptor spans slots 3-4 (selector 0x18)
        let tss_ptr = tss as *mut Tss64 as u64;
        let (lo, hi) = encode_tss_descriptor(tss_ptr, core::mem::size_of::<Tss64>() as u32 - 1);
        gdt[3] = lo;
        gdt[4] = hi;
        gdt[5] = encode_gdt_entry(0b1111_0011, 0);      // User Data (0x28)
        gdt[6] = encode_gdt_entry(0b1111_1011, 0b0010); // User Code (0x30, L-bit=1)

        // Load this CPU's GDT and TSS
        let gdt_ptr = crate::arch::gdt::GdtPointer {
            limit: (core::mem::size_of::<[u64; 10]>() - 1) as u16,
            base: gdt.as_ptr() as u64,
        };
        core::arch::asm!("lgdt [{}]", in(reg) &gdt_ptr, options(readonly, nostack, preserves_flags));
        core::arch::asm!("ltr {0:x}", in(reg) 0x18u64, options(nostack, preserves_flags));

        // Reload CS and segment registers with this CPU's GDT selectors
        core::arch::asm!(
            "push 0x08",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            out("rax") _,
            options(nostack, preserves_flags),
        );

        // Set IA32_GS_BASE (MSR 0xC0000101) to point to this CPU's PerCpu block
        crate::arch::msr::wrmsr(0xC000_0101, p as u64);
    }
}

/// Returns the number of online CPUs (BSP + APs).
pub fn cpu_count() -> usize {
    ONLINE_CPUS.load(Ordering::SeqCst) as usize
}

/// Returns the local CPU id via Local APIC MMIO register (0xFEE00020).
pub fn current_cpu() -> usize {
    if ONLINE_CPUS.load(Ordering::Relaxed) <= 1 {
        return 0;
    }
    let lapic_base = 0xFEE0_0000u64;
    let id_reg = (lapic_base + 0x20) as *const u32;
    let apic_id = unsafe { (core::ptr::read_volatile(id_reg) >> 24) as u8 };
    let per_cpu = unsafe { &*PER_CPU.get() };
    for (idx, cpu) in per_cpu.iter().enumerate() {
        if cpu.online.load(Ordering::Relaxed) && cpu.apic_id == apic_id as u64 {
            return idx;
        }
    }
    0
}


// ============================================================================
// AP trampoline (INIT-SIPI-SIPI)
// ============================================================================
//
// The real-mode trampoline is assembled via `global_asm!` below. It is copied
// to `TRAMPOLINE_BASE` (0x8000) in physical memory before sending SIPIs.
//
// The stub does:
//   1. Disables interrupts (cli)
//   2. Loads a temporary GDT (real→protected→long mode)
//   3. Enables PAE + Long Mode + Paging
//   4. Jumps to `ap_entry_64` via the function pointer stored at 0x9010

core::arch::global_asm!(
    // 16-bit real mode AP entry point
    ".section .text",
    ".global _tramp_start",
    ".global _tramp_end",
    ".code16",
    "_tramp_start:",
    "   cli",
    "   cld",
    "   xor %ax, %ax",
    "   mov %ax, %ds",
    "   mov %ax, %es",
    "   mov %ax, %ss",
    "   mov $0x8C00, %sp",              // provisional stack (grows down from 0x8C00)
    // Load temporary GDT for mode transition
    "   mov $0x8000, %bx",
    "   lgdt %ds:(_tramp_gdt_ptr - _tramp_start)(%bx)",
    // Enable Protected Mode (PE bit in CR0)
    "   mov %cr0, %eax",
    "   or $1, %eax",
    "   mov %eax, %cr0",
    // Far jump to 32-bit protected mode (selector 0x18: 32-bit code)
    "   .byte 0xea",
    "   .word 0x8000 + (_tramp_pm32 - _tramp_start)",
    "   .word 0x0018",
    ".code32",
    "_tramp_pm32:",
    // Set up data segments
    "   mov $0x10, %ax",
    "   mov %ax, %ds",
    "   mov %ax, %es",
    "   mov %ax, %ss",
    // Enable PAE (bit 5 of CR4)
    "   mov %cr4, %eax",
    "   or $0x20, %eax",
    "   mov %eax, %cr4",
    // Enable Long Mode (LME bit 8) and No-Execute (NXE bit 11) in EFER MSR
    "   mov $0xC0000080, %ecx",
    "   rdmsr",
    "   or $0x900, %eax",
    "   wrmsr",
    // Load kernel CR3 (page table root) from scratch at 0x9000
    "   mov $0x9000, %eax",
    "   mov (%eax), %eax",
    "   mov %eax, %cr3",
    // Enable Paging (PG bit in CR0)
    "   mov %cr0, %eax",
    "   or $0x80000000, %eax",
    "   mov %eax, %cr0",
    // Far jump into 64-bit Long Mode (selector 0x08: 64-bit code)
    "   .byte 0xea",
    "   .long 0x8000 + (_tramp_lm64 - _tramp_start)",
    "   .word 0x0008",
    ".code64",
    "_tramp_lm64:",
    // Load this AP's temporary stack from scratch at 0x9008
    "   mov $0x9000, %rbx",
    "   mov 8(%rbx), %rsp",
    // Call the 64-bit AP entry function at address stored at 0x9010
    "   mov 16(%rbx), %rax",
    "   call *%rax",
    // Should not return, but if it does, halt
    "   cli",
    "2:",
    "   hlt",
    "   jmp 2b",
    // Temporary GDT for mode transitions:
    //   0x00: Null descriptor
    //   0x08: 64-bit Code segment (L=1, D=0) — matches kernel CS!
    //   0x10: 32-bit Data segment
    //   0x18: 32-bit Code segment (D=1, L=0) — used for protected mode transition
    ".align 16",
    "_tramp_gdt:",
    "   .quad 0",                              // 0x00: Null descriptor
    "   .quad 0x00AF9A000000FFFF",             // 0x08: 64-bit Code segment
    "   .quad 0x00CF92000000FFFF",             // 0x10: 32-bit Data segment
    "   .quad 0x00CF9A000000FFFF",             // 0x18: 32-bit Code segment
    "_tramp_gdt_end:",
    "_tramp_gdt_ptr:",
    "   .word _tramp_gdt_end - _tramp_gdt - 1",
    "   .long 0x8000 + (_tramp_gdt - _tramp_start)",
    "_tramp_end:",
    options(att_syntax),
);

unsafe extern "C" {
    static _tramp_start: u8;
    static _tramp_end: u8;
}

/// Copies the trampoline stub into the identity-mapped low-memory region.
pub fn copy_trampoline() {
    let start = unsafe { &_tramp_start as *const u8 as u64 };
    let end = unsafe { &_tramp_end as *const u8 as u64 };
    let len = (end - start) as usize;
    if len > 0x1000 {
        panic!("[SMP] trampoline too large: {} bytes", len);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(start as *const u8, TRAMPOLINE_BASE as *mut u8, len);
    }
    crate::serial_println!("[SMP] Trampoline: {} bytes copied to {:#x}", len, TRAMPOLINE_BASE);
}

// ============================================================================
// AP 64-bit entry point (called by trampoline after mode switch)
// ============================================================================

/// CPU index scratch — written by BSP before each SIPI, read by the AP.
static AP_CPU_INDEX: AtomicU32 = AtomicU32::new(0);

/// 64-bit entry point for each Application Processor.
/// Called from the trampoline after the AP has transitioned to Long Mode.
/// The AP:
///   1. Loads its per-CPU GDT/TSS
///   2. Loads the shared IDT (read-only, safe to share)
///   3. Configures syscall MSRs
///   4. Marks itself online
///   5. Parks in a HLT idle loop (phase 1 — no per-CPU scheduler yet)
unsafe extern "C" fn ap_entry() {
    let cpu_idx = AP_CPU_INDEX.load(Ordering::SeqCst) as usize;
    let per_cpu = unsafe { &mut (*PER_CPU.get())[cpu_idx] };

    // Configure this AP's GDT/TSS/stacks
    unsafe { init_cpu_state(per_cpu as *mut PerCpu) };

    // Load the shared IDT (same table as BSP — it's read-only after init)
    unsafe {
        core::arch::asm!("lidt [{}]", in(reg) &raw const IDT_PTR_CACHE, options(readonly, nostack, preserves_flags));
    }

    // Configure syscall MSRs for this CPU
    crate::arch::syscall::init_on_cpu();

    // Enable Local APIC software-enable on this AP (SVR)
    let lapic_base: u64 = 0xFEE0_0000;
    unsafe {
        let svr_addr = (lapic_base + 0xF0) as *mut u32;
        let svr = core::ptr::read_volatile(svr_addr);
        core::ptr::write_volatile(svr_addr, svr | (1 << 8) | 0xFF);
        // Clear TPR to accept all priorities
        let tpr_addr = (lapic_base + 0x80) as *mut u32;
        core::ptr::write_volatile(tpr_addr, 0);
        // Send EOI
        let eoi_addr = (lapic_base + 0xB0) as *mut u32;
        core::ptr::write_volatile(eoi_addr, 0);
    }

    // Initialize the per-CPU scheduler run-queue for this AP
    crate::task::init_ap(cpu_idx);

    // Mark this CPU as online so BSP knows this AP has successfully booted
    per_cpu.set_online();
    ONLINE_CPUS.fetch_add(1, Ordering::SeqCst);

    // Spin-wait until BSP signals that early kernel boot is complete and SMP multitasking can start
    while !SMP_STARTED.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }

    // Initialize Local APIC periodic timer on this AP (Vector 0x40, ~100 Hz countdown)
    crate::arch::apic::init_timer(crate::arch::apic::LAPIC_TIMER_VECTOR, 1_000_000);

    // Enter the per-CPU scheduler idle loop
    crate::task::ap_idle_loop(cpu_idx);
}

// ============================================================================
// IDT pointer cache (for APs to load the same IDT as BSP)
// ============================================================================

/// Cached IDT pointer — filled by `smp::init()` from the BSP's current IDTR.
#[repr(C, packed)]
struct IdtPtrCache {
    limit: u16,
    base: u64,
}

static mut IDT_PTR_CACHE: IdtPtrCache = IdtPtrCache { limit: 0, base: 0 };

// ============================================================================
// SMP Initialization — INIT-SIPI-SIPI protocol
// ============================================================================

/// Sends an Inter-Processor Interrupt via the Local APIC's Interrupt Command
/// Register (ICR). The ICR is split across two 32-bit MMIO registers:
///   ICR_HIGH (0x310) = destination APIC ID in bits [31:24]
///   ICR_LOW  (0x300) = vector, delivery mode, level, trigger, etc.
///
/// Writing ICR_LOW triggers the IPI delivery, so ICR_HIGH must be set first.
unsafe fn send_ipi(lapic_base: u64, dest_apic_id: u8, icr_low: u32) {
    let icr_high_addr = (lapic_base + 0x310) as *mut u32;
    let icr_low_addr = (lapic_base + 0x300) as *mut u32;
    unsafe {
        // Set destination in ICR_HIGH[31:24]
        core::ptr::write_volatile(icr_high_addr, (dest_apic_id as u32) << 24);
        // Write ICR_LOW to trigger IPI
        core::ptr::write_volatile(icr_low_addr, icr_low);
    }
}

/// Waits for the ICR delivery status bit to clear (bit 12 of ICR_LOW).
unsafe fn wait_ipi_delivery(lapic_base: u64) {
    let icr_low_addr = (lapic_base + 0x300) as *const u32;
    for _ in 0..100_000 {
        let status = unsafe { core::ptr::read_volatile(icr_low_addr) };
        if (status & (1 << 12)) == 0 {
            return;
        }
        core::hint::spin_loop();
    }
}

/// Busy-wait delay loop. Each iteration is ~1 µs on a ~1 GHz CPU.
/// Used for the inter-IPI delays required by the INIT-SIPI-SIPI protocol.
fn delay_us(us: u64) {
    for _ in 0..us * 100 {
        core::hint::spin_loop();
    }
}

/// Initializes the SMP subsystem:
///   1. Caches the BSP's IDT pointer for APs to reuse
///   2. Copies the real-mode trampoline to low memory
///   3. For each AP discovered in the MADT, sends INIT-SIPI-SIPI
///   4. Waits for each AP to come online (or times out)
///
/// Must be called after `acpi::init()` and `apic::init()` on the BSP, with
/// interrupts disabled.
pub fn init() {
    // Mark BSP as online (cpu 0)
    let bsp_per_cpu = unsafe { &mut (*PER_CPU.get())[0] };
    bsp_per_cpu.cpu_id = 0;
    bsp_per_cpu.online.store(true, Ordering::SeqCst);
    ONLINE_CPUS.store(1, Ordering::SeqCst);

    // Get BSP APIC ID
    let bsp_apic_id = {
        let lapic = crate::arch::apic::LOCAL_APIC.lock();
        lapic.apic_id
    };
    bsp_per_cpu.apic_id = bsp_apic_id as u64;

    // Set IA32_GS_BASE (MSR 0xC0000101) for the BSP
    unsafe {
        crate::arch::msr::wrmsr(0xC000_0101, bsp_per_cpu as *mut PerCpu as u64);
    }


    // Read MADT core list from ACPI
    let (core_list, lapic_base) = {
        let acpi = crate::arch::acpi::ACPI_DATA.lock();
        if !acpi.is_initialized || acpi.cores.len() <= 1 {
            let core_count = if acpi.is_initialized { acpi.cores.len() } else { 1 };
            crate::serial_println!("[SMP] {} core(s) detected — SMP not needed", core_count);
            crate::println!("[OK] SMP       : {} core(s), no APs to wake.", core_count);
            return;
        }
        (acpi.cores.clone(), acpi.lapic_addr as u64)
    };

    let ap_count = core_list.iter().filter(|c| c.is_enabled && c.apic_id != bsp_apic_id).count();
    if ap_count == 0 {
        crate::serial_println!("[SMP] No enabled APs found in MADT — single-core mode");
        crate::println!("[OK] SMP       : 1 core (BSP only).");
        return;
    }

    crate::serial_println!("[SMP] BSP APIC ID: {}, {} AP(s) to wake", bsp_apic_id, ap_count);

    // Cache the BSP's IDT pointer so APs can load the same IDT
    unsafe {
        core::arch::asm!(
            "sidt [{}]",
            in(reg) &raw mut IDT_PTR_CACHE,
            options(nostack, preserves_flags)
        );
    }

    // Read kernel CR3 (page table root) for the trampoline
    let kernel_cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) kernel_cr3, options(nomem, nostack, preserves_flags));
    }

    // Copy trampoline to low memory
    copy_trampoline();

    // Wake each AP via INIT-SIPI-SIPI
    let mut cpu_idx: usize = 1; // BSP is 0
    let mut online_count: usize = 0;

    for core_info in &core_list {
        if !core_info.is_enabled || core_info.apic_id == bsp_apic_id {
            continue;
        }
        if cpu_idx >= MAX_CPUS {
            crate::serial_println!("[SMP] Warning: MAX_CPUS ({}) reached, skipping APIC ID {}", MAX_CPUS, core_info.apic_id);
            break;
        }

        // Initialize the PerCpu block for this AP
        let per_cpu = unsafe { &mut (*PER_CPU.get())[cpu_idx] };
        per_cpu.cpu_id = cpu_idx as u64;
        per_cpu.apic_id = core_info.apic_id as u64;

        // Tell the AP which CPU index it is
        AP_CPU_INDEX.store(cpu_idx as u32, Ordering::SeqCst);

        // Write scratch data at AP_SCRATCH_BASE (0x9000):
        //   [0x9000] = kernel CR3 (u32, low 32 bits — identity mapped, < 4 GiB)
        //   [0x9008] = temporary stack top (u64)
        //   [0x9010] = ap_entry function pointer (u64)
        let scratch = AP_SCRATCH_BASE as *mut u8;
        unsafe {
            // CR3 (32-bit, for the 32-bit trampoline code)
            core::ptr::write_volatile(scratch as *mut u32, kernel_cr3 as u32);
            // Temporary stack: use the top of this AP's kernel stack
            let stack_top = per_cpu.kernel_stack.as_ptr() as u64 + KERNEL_STACK_SIZE as u64;
            core::ptr::write_volatile(scratch.add(8) as *mut u64, stack_top);
            // AP entry function pointer
            core::ptr::write_volatile(scratch.add(0x10) as *mut u64, ap_entry as *const () as u64);
        }

        // SIPI vector: physical page number of trampoline (0x8000 >> 12 = 0x08)
        let sipi_vector = (TRAMPOLINE_BASE >> 12) as u8;

        crate::serial_println!("[SMP] Waking AP #{} (APIC ID {}) via INIT-SIPI-SIPI...", cpu_idx, core_info.apic_id);

        unsafe {
            // 1. Send INIT IPI (delivery mode = 0b101 = INIT, level assert)
            send_ipi(lapic_base, core_info.apic_id, 0x0000_C500);
            wait_ipi_delivery(lapic_base);

            // 2. Wait 10ms
            delay_us(10_000);

            // 3. Send INIT de-assert (level de-assert)
            send_ipi(lapic_base, core_info.apic_id, 0x0000_8500);
            wait_ipi_delivery(lapic_base);

            // 4. Wait 10ms
            delay_us(10_000);

            // 5. Send first SIPI (delivery mode = 0b110 = SIPI, vector = page)
            send_ipi(lapic_base, core_info.apic_id, 0x0000_0600 | sipi_vector as u32);
            wait_ipi_delivery(lapic_base);

            // Wait a short moment for AP to wake from first SIPI
            delay_us(1000);

            // Only send second SIPI if the AP has not come online yet
            if !per_cpu.is_online() {
                send_ipi(lapic_base, core_info.apic_id, 0x0000_0600 | sipi_vector as u32);
                wait_ipi_delivery(lapic_base);
            }
        }

        // Wait for AP to come online (timeout ~100ms)
        let mut came_online = false;
        for _ in 0..10_000 {
            if per_cpu.is_online() {
                came_online = true;
                break;
            }
            delay_us(10);
        }

        if came_online {
            online_count += 1;
            crate::serial_println!("[SMP] AP #{} (APIC ID {}) is ONLINE", cpu_idx, core_info.apic_id);
        } else {
            crate::serial_println!("[SMP] AP #{} (APIC ID {}) TIMEOUT — did not come online", cpu_idx, core_info.apic_id);
        }

        cpu_idx += 1;
    }

    let total = cpu_count();
    crate::println!("[OK] SMP       : {} CPU(s) online (BSP + {} AP(s) woken, {} responded).",
        total, ap_count, online_count);
    crate::serial_println!("[SMP] Init complete: {} CPU(s) online", total);
}
