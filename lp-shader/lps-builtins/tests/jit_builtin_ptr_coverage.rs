//! Every builtin this image links must be reachable through
//! [`lps_builtins::jit_builtin_code_ptr`] — that table *is* how the JIT resolves
//! relocations, so a missing entry is a shader that fails to link.
//!
//! Runs in the default feature configuration, i.e. **without** `float-f32`,
//! because that is the configuration device firmware builds and the one where a
//! feature-gating mistake can silently drop Fixed-mode builtins. The f32 family
//! is expected to be absent here; nothing else may be.

use lps_builtin_ids::{BuiltinId, Mode};

#[test]
fn every_non_f32_builtin_resolves_without_the_float_f32_feature() {
    let mut missing = Vec::new();
    for bid in BuiltinId::all() {
        let is_f32 = bid.mode() == Some(Mode::F32);
        let resolved = lps_builtins::jit_builtin_code_ptr(*bid).is_some();
        if !is_f32 && !resolved {
            missing.push(bid.name());
        }
        if cfg!(not(feature = "float-f32")) && is_f32 {
            assert!(
                !resolved,
                "{} resolved with `float-f32` off — the family is not linked",
                bid.name()
            );
        }
    }
    assert!(
        missing.is_empty(),
        "builtins unreachable through jit_builtin_code_ptr: {missing:?}"
    );
}

/// Distinct builtins must have distinct symbol names: the JIT's table is keyed
/// by name, so a collision silently drops whichever one is inserted first.
#[test]
fn builtin_symbol_names_are_unique() {
    let mut names: Vec<&str> = BuiltinId::all().iter().map(|b| b.name()).collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), total, "duplicate builtin symbol names");
}
