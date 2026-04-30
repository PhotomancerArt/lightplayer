//! Panic boundary helpers for node render paths (shared with `lpl-runtime`).

use crate::error::Error;

#[cfg(feature = "panic-recovery")]
use core::panic::AssertUnwindSafe;
#[cfg(feature = "panic-recovery")]
use lpc_shared::backtrace::PanicPayload;
#[cfg(feature = "panic-recovery")]
use unwinding::panic::catch_unwind;

/// Wrap a render call in catch_unwind, converting panics to Error.
#[cfg(feature = "panic-recovery")]
pub fn catch_node_panic(f: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(payload) => {
            let msg = if let Some(p) = payload.downcast_ref::<PanicPayload>() {
                p.format_error()
            } else {
                alloc::string::String::from("panic: unknown (no payload)")
            };
            Err(Error::Other { message: msg })
        }
    }
}

#[cfg(not(feature = "panic-recovery"))]
pub fn catch_node_panic(f: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
    f()
}
