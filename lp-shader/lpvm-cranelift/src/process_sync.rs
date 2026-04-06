//! Serialize Cranelift-backed LPIR codegen across threads.
//!
//! Concurrent `cranelift_jit` finalization and/or object emission has produced process crashes
//! (SIGSEGV) when `lps-filetests` runs many workers. Treat codegen as single-threaded.

#[cfg(feature = "std")]
mod imp {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static lpvm_cranelift_CODEGEN: OnceLock<Mutex<()>> = OnceLock::new();

    /// Acquire the codegen serialization lock, recovering from poison if a previous holder panicked.
    pub(crate) fn codegen_guard() -> MutexGuard<'static, ()> {
        let mutex = lpvm_cranelift_CODEGEN.get_or_init(|| Mutex::new(()));
        mutex.lock().unwrap_or_else(|poisoned| {
            mutex.clear_poison();
            poisoned.into_inner()
        })
    }
}

#[cfg(not(feature = "std"))]
mod imp {
    pub(crate) struct NoopGuard;
    impl Drop for NoopGuard {
        fn drop(&mut self) {}
    }
    pub(crate) fn codegen_guard() -> NoopGuard {
        NoopGuard
    }
}

pub(crate) use imp::codegen_guard;
