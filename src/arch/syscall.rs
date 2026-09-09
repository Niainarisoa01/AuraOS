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

/// Read a Model-Specific Register (MSR)
#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | (low as u64)
}

/// Write a Model-Specific Register (MSR)
#[inline]
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// The kernel-side syscall entry point.
///
/// When a user-space program executes `syscall`:
///   - RCX = saved RIP (return address)
///   - R11 = saved RFLAGS
///   - RAX = syscall number
///   - RDI, RSI, RDX, R10, R8, R9 = arguments
///
/// This handler dispatches to the appropriate kernel function and returns
/// the result in RAX. It then executes `sysretq` to return to user space.
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // Save user stack pointer and callee-saved registers
        // RCX = user RIP, R11 = user RFLAGS (saved by CPU)
        "swapgs",                    // Switch to kernel GS base (if applicable)
        "push rcx",                  // Save user RIP
        "push r11",                  // Save user RFLAGS
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Call the Rust syscall dispatcher
        // RAX = syscall number (already in rax)
        // RDI, RSI, RDX, R10, R8, R9 = args (R10 replaces RCX per syscall ABI)
        "mov rcx, r10",             // Restore 4th arg from R10 to RCX (C calling convention)
        "call {handler}",

        // Restore saved registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "pop r11",                   // Restore RFLAGS for sysretq
        "pop rcx",                   // Restore RIP for sysretq
        "swapgs",                    // Switch back to user GS base
        "sysretq",                   // Return to Ring 3

        handler = sym syscall_dispatch,
    );
}

/// Rust-level syscall dispatcher.
/// Called from the naked assembly entry point with the syscall number in RAX.
#[allow(unused_variables)]
extern "C" fn syscall_dispatch(
    arg1: u64,  // RDI
    arg2: u64,  // RSI
    arg3: u64,  // RDX
    arg4: u64,  // RCX (was R10)
    arg5: u64,  // R8
    arg6: u64,  // R9
) -> u64 {
    // The syscall number is in RAX, but the C ABI doesn't pass it as an argument.
    // We read it from the saved state. For now, use a simpler approach:
    // the caller will have placed syscall number in RAX before the `syscall` instruction.
    let syscall_nr: u64;
    unsafe {
        core::arch::asm!("", out("rax") syscall_nr, options(nomem, nostack));
    }

    SYSCALL_COUNT.fetch_add(1, Ordering::Relaxed);

    match syscall_nr {
        SYS_EXIT => {
            // Terminate current task
            crate::serial_println!("[SYSCALL] exit({})", arg1);
            crate::println!("[SYSCALL] Process PID exited with status {}", arg1);
            {
                let mut sched = crate::task::SCHEDULER.lock();
                let curr = sched.current;
                sched.tasks[curr].state = crate::task::TaskState::Dead;
            }
            crate::task::yield_now();
            0
        }
        SYS_WRITE => {
            // write(fd, buf_ptr, len) — for now, fd=1 → serial+VGA output
            if arg1 == 1 || arg1 == 2 {
                let ptr = arg2 as *const u8;
                let len = arg3 as usize;
                if !ptr.is_null() && len <= 4096 {
                    for i in 0..len {
                        let byte = unsafe { core::ptr::read_volatile(ptr.add(i)) };
                        if byte == 0 { break; }
                        // Output character (simplified — no actual print macro in syscall context)
                    }
                }
                len as u64
            } else {
                u64::MAX // -1 (EBADF)
            }
        }
        SYS_GETPID => {
            let sched = crate::task::SCHEDULER.lock();
            sched.tasks[sched.current].id as u64
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
                let sched = crate::task::SCHEDULER.lock();
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
                let sched = crate::task::SCHEDULER.lock();
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
            crate::serial_println!("[SYSCALL] Unknown syscall #{}", syscall_nr);
            u64::MAX // -ENOSYS
        }
    }
}

/// Initialize the syscall/sysret mechanism by configuring x86_64 MSRs.
pub fn init() {
    unsafe {
        // Step 1: Enable System Call Extensions in EFER MSR
        let efer = rdmsr(MSR_EFER);
        wrmsr(MSR_EFER, efer | EFER_SCE);

        // Step 2: Configure STAR — segment selectors
        // STAR[47:32] = kernel CS (0x08), kernel SS is CS+8 = 0x10
        // STAR[63:48] = user CS base. sysret uses this+16 for CS and this+8 for SS.
        //   User Data selector = 0x20, User Code = 0x28
        //   So base = 0x20 - 8 = 0x18? No.
        //   sysret 64-bit: CS = STAR[63:48]+16, SS = STAR[63:48]+8
        //   We want CS=0x30|3=0x33, SS=0x28|3=0x2B
        //   So STAR[63:48] = 0x20 (0x20+8=0x28, 0x20+16=0x30, RPL=3 added by CPU)
        let star = (0x08u64 << 32) | (0x20u64 << 48);
        wrmsr(MSR_STAR, star);

        // Step 3: Set LSTAR — syscall entry point address
        wrmsr(MSR_LSTAR, syscall_entry as *const () as u64);

        // Step 4: Set FMASK — mask IF on syscall entry (disable interrupts)
        wrmsr(MSR_FMASK, FMASK_VALUE);
    }

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
