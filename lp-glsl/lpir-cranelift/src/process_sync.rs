//! Serialize Cranelift-backed LPIR codegen across threads.
//!
//! Concurrent `cranelift_jit` finalization and/or object emission has produced process crashes
//! (SIGSEGV) when `lp-glsl-filetests` runs many workers. Treat codegen as single-threaded.

use std::sync::{Mutex, OnceLock};

static LPIR_CRANELIFT_CODEGEN: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn codegen_guard() -> std::sync::MutexGuard<'static, ()> {
    LPIR_CRANELIFT_CODEGEN
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("LPIR Cranelift codegen mutex poisoned")
}
