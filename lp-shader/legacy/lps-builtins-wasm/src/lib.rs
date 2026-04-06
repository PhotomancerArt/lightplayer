//! WASM module exporting all `__lp_*` builtins from `lps-builtins`.
//!
//! Built with `--import-memory` so linear memory comes from the host (same instance as the shader).
//! DCE prevention lives in `lps-builtins` (`ensure_builtins_referenced`).

/// Call once after instantiation if the loader does not otherwise retain builtin symbols.
/// The body keeps every builtin in the link graph so they appear as WASM exports.
#[unsafe(no_mangle)]
pub extern "C" fn lps_builtins_wasm_init() {
    lps_builtins::ensure_builtins_referenced();
}
