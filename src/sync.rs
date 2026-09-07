//! ============================================================================
//! Concurrency & Synchronization Primitives
//! ============================================================================
//!
//! Implements a lightweight, bare-metal atomic Spinlock with interrupt safety
//! (disables CPU interrupts while holding lock to prevent ISR deadlocks)
//! and AtomicBool with Acquire / Release memory ordering barriers.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// Reads RFLAGS, tests if interrupts were enabled (bit 9, IF), and disables them via `cli`.
#[inline]
fn pushfq_and_cli() -> bool {
    let rflags: u64;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {}",
            "cli",
            out(reg) rflags,
        );
    }
    // IF flag is bit 9
    (rflags & (1 << 9)) != 0
}

/// Restores interrupts via `sti` if they were previously enabled.
#[inline]
fn restore_interrupts(enabled: bool) {
    if enabled {
        unsafe {
            core::arch::asm!("sti", options(nomem, nostack));
        }
    }
}

pub struct Spinlock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Acquires the lock, saving the current interrupt state and disabling interrupts
    /// until the guard is dropped.
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        let interrupts_enabled = pushfq_and_cli();
        while self.lock.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        SpinlockGuard {
            spinlock: self,
            interrupts_enabled,
        }
    }

    /// Attempts to acquire the lock without spinning.
    #[allow(dead_code)]
    pub fn try_lock(&self) -> Option<SpinlockGuard<'_, T>> {
        let interrupts_enabled = pushfq_and_cli();
        if !self.lock.swap(true, Ordering::Acquire) {
            Some(SpinlockGuard {
                spinlock: self,
                interrupts_enabled,
            })
        } else {
            restore_interrupts(interrupts_enabled);
            None
        }
    }

    /// Forcibly releases the lock. Used primarily in panic handlers to avoid secondary deadlocks.
    pub unsafe fn force_unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }
}

pub struct SpinlockGuard<'a, T> {
    spinlock: &'a Spinlock<T>,
    interrupts_enabled: bool,
}

impl<'a, T> Deref for SpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.spinlock.data.get() }
    }
}

impl<'a, T> DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.spinlock.data.get() }
    }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.spinlock.lock.store(false, Ordering::Release);
        restore_interrupts(self.interrupts_enabled);
    }
}
