//! Serialize Cranelift-backed LPIR codegen across threads.
//!
//! Concurrent `cranelift_jit` finalization and/or object emission has produced process crashes
//! (SIGSEGV) when `lp-glsl-filetests` runs many workers. Treat codegen as single-threaded.

#[cfg(feature = "std")]
mod imp {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static LPIR_CRANELIFT_CODEGEN: OnceLock<Mutex<()>> = OnceLock::new();

    pub(crate) fn codegen_guard() -> MutexGuard<'static, ()> {
        LPIR_CRANELIFT_CODEGEN
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("LPIR Cranelift codegen mutex poisoned")
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
