//! ============================================================================
//! Ring 3 — User Privilege Level Transitions (iretq)
//! ============================================================================
//!
//! Provides bare-metal CPU operations to switch from Ring 0 (Kernel) to Ring 3
//! (User space) using the x86_64 `iretq` instruction.
//!
//! Privilege Level 3 Execution Frame (pushed to stack before `iretq`):
//!   [RSP + 32] = SS     (User Data Segment: 0x20 | 3 = 0x23)
//!   [RSP + 24] = RSP    (User Stack Pointer)
//!   [RSP + 16] = RFLAGS (0x202 = Interrupts Enabled)
//!   [RSP + 8]  = CS     (User Code Segment: 0x28 | 3 = 0x2B)
//!   [RSP + 0]  = RIP    (User Entry Point)

use crate::arch::gdt::{USER_CS, USER_DS};

/// Transitions the CPU to Ring 3 and begins execution at `entry_point`
/// with the provided user stack.
///
/// # Safety
/// The caller must ensure that:
/// 1. `entry_point` points to valid executable memory mapped with `USER_ACCESSIBLE`.
/// 2. `user_stack_top` points to valid stack memory mapped with `USER_ACCESSIBLE` and `WRITABLE`.
/// 3. TSS.rsp0 is configured with a valid kernel stack to handle exceptions/interrupts/syscalls.
#[allow(dead_code)]
#[inline(never)]
pub unsafe fn enter_user_mode(entry_point: u64, user_stack_top: u64) -> ! {
    let user_cs = USER_CS as u64;
    let user_ds = USER_DS as u64;
    let rflags = 0x202u64; // IF=1, reserved bit 1 = 1

    unsafe {
        core::arch::asm!(
            // Reload data segment registers with User Data selector (0x23)
            "mov ds, cx",
            "mov es, cx",
            "mov fs, cx",
            "mov gs, cx",

            // Push iretq stack frame:
            "push rcx", // SS (User Data: 0x23)
            "push rsi", // RSP (user_stack_top)
            "push rdx", // RFLAGS (0x202)
            "push r8",  // CS (User Code: 0x2B)
            "push rdi", // RIP (entry_point)

            // Clear general-purpose registers before entering user space for isolation
            "xor rax, rax",
            "xor rbx, rbx",
            "xor rcx, rcx",
            "xor rdx, rdx",
            "xor rsi, rsi",
            "xor rdi, rdi",
            "xor rbp, rbp",
            "xor r8,  r8",
            "xor r9,  r9",
            "xor r10, r10",
            "xor r11, r11",
            "xor r12, r12",
            "xor r13, r13",
            "xor r14, r14",
            "xor r15, r15",

            // Execute iretq: pops RIP, CS, RFLAGS, RSP, SS and switches CPL to 3
            "iretq",

            in("rdi") entry_point,
            in("rsi") user_stack_top,
            in("rdx") rflags,
            in("r8") user_cs,
            in("rcx") user_ds,
            options(noreturn)
        );
    }
}

/// Validates that a given iretq frame would transition to Ring 3.
/// Returns (cs_ok, ss_ok, rflags_ok).
#[allow(dead_code)]
pub fn validate_ring3_frame(cs: u16, ss: u16, rflags: u64) -> (bool, bool, bool) {
    let cs_ok = (cs & 3) == 3 && cs == USER_CS;
    let ss_ok = (ss & 3) == 3 && ss == USER_DS;
    let rflags_ok = (rflags & 0x200) != 0; // IF enabled
    (cs_ok, ss_ok, rflags_ok)
}
