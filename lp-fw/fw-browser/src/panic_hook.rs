//! Primary-panic capture for the panic=abort instance.
//!
//! Under `panic=abort` (per-target panic-strategy ADR) a panic inside a
//! wasm export becomes an `unreachable` trap: JavaScript sees only a
//! generic `WebAssembly.RuntimeError`, the panic MESSAGE never leaves the
//! instance, and after the abort the instance cannot be safely re-entered
//! to ask for it (Rust drops were skipped, so e.g. the runtime registry's
//! `RefCell` borrow flag is leaked — the poisoned-instance defect).
//!
//! The panic hook is the one place the message can escape: it runs before
//! the abort, while the instance is still coherent. It mirrors the panic
//! to the worker console and stashes it on the worker global scope
//! ([`LAST_PANIC_GLOBAL`]), where the worker script's instance-fatal
//! handler reads it back WITHOUT calling into the poisoned instance.

use std::sync::Once;

use wasm_bindgen::JsValue;

/// Property name on the worker global scope (`self`) that receives the
/// formatted panic. The worker script deletes it after reading, so a
/// stale value can never be attributed to a later failure.
const LAST_PANIC_GLOBAL: &str = "__lp_last_panic";

/// Install the panic hook. Idempotent; installed alongside the logger so
/// every export entry point is covered.
pub fn install() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            // `PanicHookInfo`'s Display carries payload and location.
            let message = info.to_string();
            crate::logger::console_error(&format!("[fw-browser] {message}"));
            // Best-effort: a failed Reflect write must not panic inside
            // the hook (that would abort with no message at all).
            let _ = js_sys::Reflect::set(
                &js_sys::global(),
                &JsValue::from_str(LAST_PANIC_GLOBAL),
                &JsValue::from_str(&message),
            );
        }));
    });
}
