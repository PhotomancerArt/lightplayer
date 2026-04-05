//! WASM module exporting all `__lp_*` builtins from `lp-glsl-builtins`.
//!
//! Built with `--import-memory` so linear memory comes from the host (same instance as the shader).
//! Regenerate `builtin_refs.rs` via `scripts/build-builtins.sh` or `lp-glsl-builtins-gen-app`.

mod builtin_refs;

/// Call once after instantiation if the loader does not otherwise retain builtin symbols.
/// The body keeps every builtin in the link graph so they appear as WASM exports.
#[unsafe(no_mangle)]
pub extern "C" fn lp_glsl_builtins_wasm_init() {
    builtin_refs::ensure_builtins_referenced();
}
