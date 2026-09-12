//! ============================================================================
//! Syscall — x86_64 Native System Call Interface (syscall/sysret)
//! ============================================================================
//!
//! Configures the AMD64 `syscall`/`sysret` fast system call mechanism using
//! Model-Specific Registers (MSRs). This provides a low-overhead transition
//! from Ring 3 (user space) to Ring 0 (kernel) without the overhead of
//! software interrupts.
//!
//! MSRs used:
//!   EFER  (0xC0000080) — Enable System Call Extensions (SCE bit)
//!   STAR  (0xC0000081) — Segment selectors for syscall/sysret
//!   LSTAR (0xC0000082) — RIP of the kernel syscall entry point
//!   FMASK (0xC0000084) — RFLAGS mask applied on syscall (clears IF)
//!
//! Syscall convention (Linux-compatible):
//!   RAX = syscall number
//!   RDI, RSI, RDX, R10, R8, R9 = arguments 1-6
//!   RAX = return value (negative for error)

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Indicates whether syscall MSRs have been configured
pub static SYSCALL_CONFIGURED: AtomicBool = AtomicBool::new(false);

/// Counter for total syscalls invoked (for diagnostics)
pub static SYSCALL_COUNT: AtomicU64 = AtomicU64::new(0);

// MSR addresses
const MSR_EFER: u32 = 0xC000_0080;
const MSR_STAR: u32 = 0xC000_0081;
const MSR_LSTAR: u32 = 0xC000_0082;
const MSR_FMASK: u32 = 0xC000_0084;

// EFER bits
const EFER_SCE: u64 = 1 << 0; // System Call Extensions

// RFLAGS bits to mask on syscall entry (clear IF to disable interrupts)
const FMASK_VALUE: u64 = 0x200; // Mask IF (bit 9)

// Syscall numbers (Linux-compatible subset)
#[allow(dead_code)]
pub const SYS_EXIT: u64 = 1;
#[allow(dead_code)]
pub const SYS_FORK: u64 = 2;
#[allow(dead_code)]
pub const SYS_READ: u64 = 3;
#[allow(dead_code)]
pub const SYS_WRITE: u64 = 4;
#[allow(dead_code)]
pub const SYS_OPEN: u64 = 5;
#[allow(dead_code)]
pub const SYS_CLOSE: u64 = 6;
#[allow(dead_code)]
pub const SYS_MMAP: u64 = 9;
#[allow(dead_code)]
pub const SYS_MUNMAP: u64 = 11;
#[allow(dead_code)]
pub const SYS_YIELD: u64 = 24;
#[allow(dead_code)]
pub const SYS_SLEEP: u64 = 35;
#[allow(dead_code)]
pub const SYS_GETPID: u64 = 39;
#[allow(dead_code)]
pub const SYS_SEND: u64 = 60;
#[allow(dead_code)]
pub const SYS_RECV: u64 = 61;
#[allow(dead_code)]
pub const SYS_TIME: u64 = 201;

use crate::arch::msr::{rdmsr, wrmsr};

/// Scratch variables for switching between user and kernel stacks on syscall.
/// Using AtomicU64 instead of `static mut` for future SMP safety.
pub static USER_RSP_SCRATCH: AtomicU64 = AtomicU64::new(0);
pub static KERNEL_RSP_SCRATCH: AtomicU64 = AtomicU64::new(0);

/// Scratch slot for the syscall number (RAX on entry).
///
/// The `syscall_entry` naked assembly saves RAX here *before* calling the Rust
/// dispatcher. Reading the number from the 7th stack argument instead is
/// unreliable: the compiler's prologue moves RSP before the read, so a direct
/// `[rsp + 8]` access lands in uninitialized local space (observed as bogus
/// syscall numbers). FMASK clears IF on entry and the read happens before any
/// preemption point, so a single global slot is safe on this UP kernel (a
/// future SMP port should make it per-CPU).
static SYSCALL_NR_SCRATCH: AtomicU64 = AtomicU64::new(0);

/// Updates the kernel stack pointer used on syscall entry
pub fn set_kernel_rsp(rsp: u64) {
    KERNEL_RSP_SCRATCH.store(rsp, Ordering::Release);
}

/// The kernel-side syscall entry point.
///
/// When a user-space program executes `syscall`:
///   - RCX = saved RIP (return address)
///   - R11 = saved RFLAGS
///   - RAX = syscall number
///   - RDI, RSI, RDX, R10, R8, R9 = arguments
///   - RSP = user stack pointer (must switch to kernel stack immediately!)
///
/// This handler switches to the task's kernel stack, dispatches to the
/// appropriate kernel function, restores the user stack, and executes `sysretq`.
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // 1. Save user RSP to scratch variable
        "mov [rip + {user_rsp}], rsp",
        // 2. Switch to the task's dedicated kernel stack
        "mov rsp, [rip + {kernel_rsp}]",

        // 3. Save user RSP, user RIP, user RFLAGS, and callee-saved registers on kernel stack
        "push [rip + {user_rsp}]",   // User RSP
        "push rcx",                  // User RIP (saved by CPU)
        "push r11",                  // User RFLAGS (saved by CPU)
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // 4. Push syscall number (RAX) onto kernel stack as 7th argument
        //    (kept for stack balance below), and snapshot it into a scratch
        //    static so the Rust dispatcher reads it reliably regardless of
        //    the compiler's stack frame layout.
        "push rax",
        "mov [rip + {nr}], rax",

        // 5. Call the Rust syscall dispatcher
        // RDI, RSI, RDX, R10, R8, R9 = args (R10 replaces RCX per syscall ABI)
        "mov rcx, r10",             // Restore 4th arg from R10 to RCX (C calling convention)
        "call {handler}",

        // 6. Pop the saved syscall number (balance the push)
        "add rsp, 8",

        // 7. Restore callee-saved registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "pop r11",                   // Restore user RFLAGS for sysretq
        "pop rcx",                   // Restore user RIP for sysretq
        "pop rsp",                   // Restore user RSP!
        "sysretq",                   // Return to Ring 3

        user_rsp = sym USER_RSP_SCRATCH,
        kernel_rsp = sym KERNEL_RSP_SCRATCH,
        nr = sym SYSCALL_NR_SCRATCH,
        handler = sym syscall_dispatch,
    );
}

/// Rust-level syscall dispatcher.
/// Called from the naked assembly entry point. User arguments arrive in the
/// first six registers (RDI/RSI/RDX/RCX/R8/R9); the syscall number is read
/// from `SYSCALL_NR_SCRATCH`, which the entry assembly snapshots from RAX
/// before the call. Reading it from the stack (7th-arg convention) is not
/// reliable here: the compiled prologue relocates RSP before the read.
#[allow(unused_variables)]
extern "C" fn syscall_dispatch(
    arg1: u64,  // RDI
    arg2: u64,  // RSI
    arg3: u64,  // RDX
    arg4: u64,  // RCX (was R10)
    arg5: u64,  // R8
    arg6: u64,  // R9
) -> u64 {
    // Syscall number captured from RAX by the naked entry before calling us.
    let syscall_nr = SYSCALL_NR_SCRATCH.load(Ordering::Relaxed);

    SYSCALL_COUNT.fetch_add(1, Ordering::Relaxed);

    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };

    match syscall_nr {
        SYS_EXIT => {
            // Terminate current task
            crate::klog!(Info, "syscall", "exit({})", arg1);
            crate::println!("[SYSCALL] Process PID exited with status {}", arg1);
            {
                let mut sched = crate::task::CPU_SCHEDULERS[cpu_id].lock();
                let curr = sched.current;
                sched.tasks[curr].state = crate::task::TaskState::Dead;
            }
            crate::task::yield_now();
            0
        }
        SYS_WRITE => {
            // write(fd, buf_ptr, len) — fd=1 (stdout), fd=2 (stderr)
            // I1: Serial output goes through per-CPU buffer (no global SERIAL1 lock).
            //     VGA WRITER lock is still needed (single framebuffer device).
            if arg1 == 1 || arg1 == 2 {
                let ptr = arg2 as *const u8;
                let len = arg3 as usize;
                if !ptr.is_null() && len <= 4096 {
                    // Build a temporary buffer for the serial per-CPU write
                    let mut vga = crate::drivers::vga::WRITER.lock();
                    for i in 0..len {
                        let byte = unsafe { core::ptr::read_volatile(ptr.add(i)) };
                        if byte == 0 { break; }
                        vga.write_byte(byte);
                    }
                    drop(vga);
                    // Serial output through per-CPU buffer
                    for i in 0..len {
                        let byte = unsafe { core::ptr::read_volatile(ptr.add(i)) };
                        if byte == 0 { break; }
                        // Use serial_print which writes to per-CPU buffer
                        if byte == b'\n' {
                            crate::serial_print!("\n");
                        } else if byte >= 0x20 && byte <= 0x7E {
                            crate::serial_print!("{}", byte as char);
                        } else {
                            crate::serial_print!("{}", byte as char);
                        }
                    }
                }
                len as u64
            } else {
                u64::MAX // -1 (EBADF)
            }
        }
        SYS_GETPID => {
            let sched = crate::task::CPU_SCHEDULERS[cpu_id].lock();
            sched.tasks[sched.current].id as u64
        }
        SYS_MMAP => {
            let addr_hint = if arg1 != 0 { Some(arg1) } else { None };
            let length = arg2 as usize;
            let prot = arg3 as u32;
            let flags = arg4 as u32;

            let mut sched = crate::task::CPU_SCHEDULERS[cpu_id].lock();
            let curr = sched.current;
            if curr < sched.tasks.len() {
                if let Some(ref mut space) = sched.tasks[curr].address_space {
                    match space.mmap(addr_hint, length, prot, flags) {
                        Ok(mapped_vaddr) => mapped_vaddr,
                        Err(_) => u64::MAX, // -1 (MAP_FAILED)
                    }
                } else {
                    u64::MAX
                }
            } else {
                u64::MAX
            }
        }
        SYS_MUNMAP => {
            let addr = arg1;
            let length = arg2 as usize;

            let mut sched = crate::task::CPU_SCHEDULERS[cpu_id].lock();
            let curr = sched.current;
            if curr < sched.tasks.len() {
                if let Some(ref mut space) = sched.tasks[curr].address_space {
                    match space.munmap(addr, length) {
                        Ok(()) => 0,
                        Err(_) => u64::MAX, // -1 (EINVAL)
                    }
                } else {
                    u64::MAX
                }
            } else {
                u64::MAX
            }
        }
        SYS_YIELD => {
            let _ = syscall_nr; // unused after this
            crate::task::yield_now();
            0
        }
        SYS_SLEEP => {
            crate::task::sleep_ms(arg1);
            0
        }
        SYS_TIME => {
            let rtc = crate::drivers::cmos::read_rtc();
            // Pack as Unix-style timestamp approximation
            // (simplified: return raw RTC values packed)
            ((rtc.hour as u64) << 16) | ((rtc.minute as u64) << 8) | (rtc.second as u64)
        }
        SYS_SEND => {
            let target_pid = arg1 as usize;
            let msg_type = arg2 as u32;
            let len = core::cmp::min(arg4 as usize, crate::task::ipc::IPC_PAYLOAD_MAX);
            let my_pid = {
                let sched = crate::task::CPU_SCHEDULERS[cpu_id].lock();
                sched.tasks[sched.current].id
            };

            let data_slice: &[u8] = if arg3 != 0 && len > 0 {
                unsafe { core::slice::from_raw_parts(arg3 as *const u8, len) }
            } else {
                &[]
            };

            if crate::task::ipc::send_message(my_pid, target_pid, msg_type, data_slice) {
                0
            } else {
                u64::MAX // queue full or error
            }
        }
        SYS_RECV => {
            let my_pid = {
                let sched = crate::task::CPU_SCHEDULERS[cpu_id].lock();
                sched.tasks[sched.current].id
            };
            if let Some(msg) = crate::task::ipc::receive_message(my_pid) {
                if arg1 != 0 {
                    unsafe {
                        let out_ptr = arg1 as *mut crate::task::ipc::Message;
                        core::ptr::write(out_ptr, msg);
                    }
                }
                1 // 1 message delivered
            } else {
                0 // 0 messages waiting
            }
        }
        _ => {
            crate::klog!(Warn, "syscall", "Unknown syscall #{}", syscall_nr);
            u64::MAX // -ENOSYS
        }
    }
}

/// Programmatic invocation helper for kernel diagnostics, unit tests, and self-tests.
pub fn dispatch_syscall(
    nr: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> u64 {
    SYSCALL_NR_SCRATCH.store(nr, Ordering::SeqCst);
    syscall_dispatch(arg1, arg2, arg3, arg4, arg5, arg6)
}

/// Configures the syscall/sysret MSRs for the CURRENT CPU.
/// This is the low-level primitive called by both the BSP's `init()` and each
/// AP's startup path. It does not print boot messages.
pub fn init_on_cpu() {
    unsafe {
        // Enable System Call Extensions in EFER MSR
        let efer = rdmsr(MSR_EFER);
        wrmsr(MSR_EFER, efer | EFER_SCE);

        // Configure STAR — segment selectors
        let star = (0x08u64 << 32) | (0x20u64 << 48);
        wrmsr(MSR_STAR, star);

        // Set LSTAR — syscall entry point address
        wrmsr(MSR_LSTAR, syscall_entry as *const () as u64);

        // Set FMASK — mask IF on syscall entry (disable interrupts)
        wrmsr(MSR_FMASK, FMASK_VALUE);
    }
}

/// Initialize the syscall/sysret mechanism by configuring x86_64 MSRs.
pub fn init() {
    init_on_cpu();

    SYSCALL_CONFIGURED.store(true, Ordering::SeqCst);

    crate::println!("[OK] Syscall   : MSRs configured (EFER.SCE, STAR, LSTAR, FMASK).");
    crate::serial_println!("[OK] Syscall: syscall/sysret MSRs configured.");
}

/// Read the current EFER MSR value (for test verification)
#[allow(dead_code)]
pub fn read_efer() -> u64 {
    unsafe { rdmsr(MSR_EFER) }
}

/// Read the current STAR MSR value (for test verification)
#[allow(dead_code)]
pub fn read_star() -> u64 {
    unsafe { rdmsr(MSR_STAR) }
}

/// Read the current LSTAR MSR value (for test verification)
#[allow(dead_code)]
pub fn read_lstar() -> u64 {
    unsafe { rdmsr(MSR_LSTAR) }
}

/// Read the current FMASK MSR value (for test verification)
#[allow(dead_code)]
pub fn read_fmask() -> u64 {
    unsafe { rdmsr(MSR_FMASK) }
}
