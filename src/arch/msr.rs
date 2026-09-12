//! ============================================================================
//! Model-Specific Register (MSR) Access Primitives
//! ============================================================================
//!
//! Provides safe wrappers around the x86_64 `rdmsr` and `wrmsr` instructions
//! for reading and writing Model-Specific Registers. Used by the APIC, syscall,
//! and other CPU configuration subsystems.

/// Reads a 64-bit Model-Specific Register (MSR).
///
/// # Safety
/// The caller must ensure that `msr` is a valid MSR address for the current CPU.
#[inline]
pub unsafe fn rdmsr(msr: u32) -> u64 {
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

/// Writes a 64-bit value to a Model-Specific Register (MSR).
///
/// # Safety
/// The caller must ensure that `msr` is a valid MSR address and `value` is a
/// valid configuration for the target MSR.
#[inline]
pub unsafe fn wrmsr(msr: u32, value: u64) {
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
