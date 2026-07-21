//! The allocation guard: a development instrument, not a safety net.
//!
//! A global allocator wrapper plus a thread-local "this thread is real-time"
//! flag. When the flag is set, an allocation panics with a backtrace pointing at
//! whoever allocated.
//!
//! **On in debug and test builds; compiled out in release** (eng-01 §10). A
//! panic in the audio callback is worse than the allocation it caught, so the
//! guard exists to fail loudly during development and to be absent in the field.
//! The release policy for a fault in the callback is different and stated in
//! [`crate::engine`]: never panic — silence, and a record.
//!
//! The `OfflineDriver` arms it too, which is the point of a driver-agnostic core:
//! **CI enforces the real-time discipline on a machine with no real time.**

/// Arm the guard for the duration of a scope, and disarm it however the scope
/// exits.
pub struct RtScope(());

impl RtScope {
    pub fn enter() -> RtScope {
        set(true);
        RtScope(())
    }
}

impl Drop for RtScope {
    fn drop(&mut self) {
        set(false);
    }
}

#[cfg(debug_assertions)]
mod armed {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    // `const` initialization matters: a lazily-initialized thread-local would
    // allocate on first touch — inside the allocator — and a thread-local with a
    // destructor would register one, which also allocates. A `Cell<bool>` has
    // neither.
    thread_local! {
        static REALTIME: Cell<bool> = const { Cell::new(false) };
    }

    pub fn set(on: bool) {
        REALTIME.with(|flag| flag.set(on));
    }

    pub fn is_realtime() -> bool {
        REALTIME.try_with(|flag| flag.get()).unwrap_or(false)
    }

    struct Guard;

    impl Guard {
        fn check(&self, what: &str, size: usize) {
            // Unwinding allocates — the panic payload and its message both do.
            // Without this exemption *any* panic inside an armed scope would
            // trip the guard while the first panic was still in flight, and a
            // double panic aborts: the process would die with no message at all
            // instead of the readable one it was about to print. Found by the
            // guard's own unwind test, which is the only reason it is here.
            if is_realtime() && !std::thread::panicking() {
                // Disarm before panicking: unwinding allocates, and a guard that
                // re-entered itself would abort with a useless message instead
                // of the backtrace that is the whole point.
                set(false);
                panic!(
                    "allocation on the real-time thread: {what} of {size} bytes. \
                     The callback may not allocate (eng-01 §2)."
                );
            }
        }
    }

    // SAFETY: every method forwards to `System` unchanged; the guard only
    // observes.
    unsafe impl GlobalAlloc for Guard {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            self.check("alloc", layout.size());
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            self.check("dealloc", layout.size());
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            self.check("realloc", new_size);
            unsafe { System.realloc(ptr, layout, new_size) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            self.check("alloc_zeroed", layout.size());
            unsafe { System.alloc_zeroed(layout) }
        }
    }

    #[global_allocator]
    static GUARD: Guard = Guard;
}

#[cfg(not(debug_assertions))]
mod armed {
    pub fn set(_on: bool) {}
    pub fn is_realtime() -> bool {
        false
    }
}

pub use armed::{is_realtime, set};

#[cfg(test)]
mod test;
