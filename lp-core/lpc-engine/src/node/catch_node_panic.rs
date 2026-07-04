//! Panic boundary helpers for node execution paths.

#[cfg(feature = "panic-recovery")]
use core::panic::AssertUnwindSafe;
#[cfg(feature = "panic-recovery")]
use lpc_shared::backtrace::PanicPayload;
#[cfg(feature = "panic-recovery")]
use unwinding::panic::catch_unwind;

use super::NodeError;

/// Wrap a node call in `catch_unwind`, converting panics to [`NodeError`].
#[cfg(feature = "panic-recovery")]
pub fn catch_node_panic<T>(f: impl FnOnce() -> Result<T, NodeError>) -> Result<T, NodeError> {
    match catch_panic("panic during node execution", f) {
        Ok(result) => result,
        Err(message) => Err(NodeError::msg(message)),
    }
}

/// [`catch_node_panic`] inside a recovery frame.
///
/// Enters a crash-recovery frame for the duration of `f`: the persistent
/// frame stack then blames this work if the device panics hard or hangs
/// (watchdog), and paths gated red after repeated crashes are denied up
/// front with a user-legible [`NodeError`] instead of executing.
///
/// On targets without an installed recovery global this is exactly
/// `catch_node_panic` (inert frame guard).
pub fn catch_node_panic_framed<T>(
    kind: lp_recovery::FrameKind,
    name: &str,
    f: impl FnOnce() -> Result<T, NodeError>,
) -> Result<T, NodeError> {
    let _guard = match lp_recovery::enter(kind, name) {
        Ok(guard) => guard,
        Err(denied) => return Err(NodeError::msg(alloc::format!("{denied}"))),
    };
    catch_node_panic(f)
}

#[cfg(not(feature = "panic-recovery"))]
pub fn catch_node_panic<T>(f: impl FnOnce() -> Result<T, NodeError>) -> Result<T, NodeError> {
    f()
}

/// Wrap arbitrary node-owned work in `catch_unwind`, returning formatted panic text.
#[cfg(feature = "panic-recovery")]
pub fn catch_panic<T>(
    fallback: &'static str,
    f: impl FnOnce() -> T,
) -> Result<T, alloc::string::String> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => Ok(result),
        Err(payload) => {
            // Layer-1 recovery caught this panic: void the staged breadcrumb
            // (no reboot will happen) and feed the crash into the blame
            // ledger so repeat offenders get gated.
            lp_recovery::record_recovered_crash();
            Err(format_panic_payload(&payload, fallback))
        }
    }
}

#[cfg(not(feature = "panic-recovery"))]
pub fn catch_panic<T>(
    _fallback: &'static str,
    f: impl FnOnce() -> T,
) -> Result<T, alloc::string::String> {
    Ok(f())
}

#[cfg(feature = "panic-recovery")]
fn format_panic_payload(
    payload: &alloc::boxed::Box<dyn core::any::Any + Send>,
    fallback: &'static str,
) -> alloc::string::String {
    if let Some(p) = payload.downcast_ref::<PanicPayload>() {
        p.format_error()
    } else if let Some(message) = payload.downcast_ref::<alloc::string::String>() {
        alloc::format!("panic: {message}")
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        alloc::format!("panic: {message}")
    } else {
        alloc::string::String::from(fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn catch_node_panic_passes_regular_errors_through() {
        let err =
            catch_node_panic(|| -> Result<(), NodeError> { Err(NodeError::msg("node failed")) })
                .expect_err("node error");

        assert_eq!(err.to_string(), "node failed");
    }

    #[cfg(feature = "panic-recovery")]
    #[test]
    fn catch_node_panic_formats_panic_payload() {
        let err = catch_node_panic(|| -> Result<(), NodeError> {
            let payload: alloc::boxed::Box<dyn core::any::Any + Send> =
                alloc::boxed::Box::new(lpc_shared::backtrace::PanicPayload::new(
                    "compiler fell over",
                    Some("shader.rs"),
                    Some(42),
                ));
            let _code = unwinding::panic::begin_panic(payload);
            unreachable!("begin_panic should unwind to catch_node_panic");
        })
        .expect_err("panic should be caught");

        let message = err.to_string();
        assert!(message.contains("panic: compiler fell over"));
        assert!(message.contains("shader.rs:42"));
    }
}
