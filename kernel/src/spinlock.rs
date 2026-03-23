//! SMP-safe spinlock with IRQ save/restore.
//!
//! `SpinLock<T>` — ticket-based spinlock that disables interrupts while held.
//! This prevents deadlocks from IRQ handlers trying to acquire a lock
//! already held by the interrupted code.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

/// A ticket-based spinlock protecting data of type `T`.
///
/// Acquiring the lock disables interrupts (cli). Releasing restores
/// the previous interrupt flag state.
pub struct SpinLock<T> {
    next_ticket: AtomicU32,
    now_serving: AtomicU32,
    data: UnsafeCell<T>,
}

// SAFETY: SpinLock serializes all access.
unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

impl<T> SpinLock<T> {
    /// Create a new unlocked spinlock.
    pub const fn new(value: T) -> Self {
        Self {
            next_ticket: AtomicU32::new(0),
            now_serving: AtomicU32::new(0),
            data: UnsafeCell::new(value),
        }
    }

    /// Acquire the lock. Disables interrupts and spins until the lock is free.
    /// Returns a guard that auto-releases on drop.
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        // Save current interrupt state and disable interrupts
        let rflags = save_and_disable_interrupts();

        // Take a ticket
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);

        // Spin until it's our turn
        while self.now_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }

        SpinLockGuard {
            lock: self,
            saved_rflags: rflags,
        }
    }

    /// Try to acquire the lock without blocking.
    /// Returns `None` if the lock is already held.
    #[allow(dead_code)]
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        let rflags = save_and_disable_interrupts();

        let current = self.now_serving.load(Ordering::Relaxed);
        let result = self.next_ticket.compare_exchange(
            current,
            current + 1,
            Ordering::Acquire,
            Ordering::Relaxed,
        );

        match result {
            Ok(_) => Some(SpinLockGuard {
                lock: self,
                saved_rflags: rflags,
            }),
            Err(_) => {
                // Failed to acquire — restore interrupt state
                restore_interrupts(rflags);
                None
            }
        }
    }
}

/// RAII guard for SpinLock. Releases the lock and restores interrupts on drop.
pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
    saved_rflags: u64,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        // Release: advance the serving counter
        self.lock.now_serving.fetch_add(1, Ordering::Release);

        // Restore previous interrupt state
        restore_interrupts(self.saved_rflags);
    }
}

// ---------------------------------------------------------------------------
// IRQ save / restore helpers
// ---------------------------------------------------------------------------

/// Save RFLAGS and disable interrupts (cli).
#[inline(always)]
fn save_and_disable_interrupts() -> u64 {
    let rflags: u64;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {}",
            "cli",
            out(reg) rflags,
            options(nomem, preserves_flags),
        );
    }
    rflags
}

/// Restore RFLAGS (re-enables interrupts if they were enabled before).
#[inline(always)]
fn restore_interrupts(rflags: u64) {
    if rflags & 0x200 != 0 {
        // IF was set — re-enable interrupts
        unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    }
}
