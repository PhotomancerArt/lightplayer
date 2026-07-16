//! Editor-facing inventory of the GLSL builtins this frontend **inlines**.
//!
//! These builtins lower directly to LPIR (`BuiltinKind` in the type
//! checker) — they have no `BuiltinId` and never appear in the runtime
//! dispatch tables, so the generated completion manifest
//! (`lps-builtin-completions`) cannot discover them from the builtin
//! sources. This table is their declaration **as data**, kept beside the
//! compiler so it cannot drift: the tests below tie every entry to
//! [`crate::hir::builtin_kind`] and the compiler's own arity check, and
//! the manifest generator consumes [`INLINED_BUILTINS`] directly (roadmap
//! D5: completions are generated from the compiler's builtin source).
//!
//! Parameter names are the GLSL-spec conventional names; they exist for
//! completion snippets only and carry no semantics. The list is sorted by
//! name.

/// One inlined GLSL builtin: its callable name and the GLSL-spec
/// parameter names (whose count is the accepted arity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlinedBuiltin {
    /// The GLSL-facing callable name, e.g. `"mix"`.
    pub name: &'static str,
    /// Conventional parameter names; `len()` is the accepted argument
    /// count. Out-params (e.g. `modf`'s `i`) are listed like the rest.
    pub params: &'static [&'static str],
}

const fn entry(name: &'static str, params: &'static [&'static str]) -> InlinedBuiltin {
    InlinedBuiltin { name, params }
}

/// Every builtin the frontend inlines, sorted by name.
pub const INLINED_BUILTINS: &[InlinedBuiltin] = &[
    entry("abs", &["x"]),
    entry("all", &["x"]),
    entry("any", &["x"]),
    entry("bitCount", &["value"]),
    entry("bitfieldExtract", &["value", "offset", "bits"]),
    entry("bitfieldInsert", &["base", "insert", "offset", "bits"]),
    entry("bitfieldReverse", &["value"]),
    entry("ceil", &["x"]),
    entry("clamp", &["x", "minVal", "maxVal"]),
    entry("cross", &["x", "y"]),
    entry("degrees", &["radians"]),
    entry("determinant", &["m"]),
    entry("distance", &["p0", "p1"]),
    entry("dot", &["x", "y"]),
    entry("equal", &["x", "y"]),
    entry("findLSB", &["value"]),
    entry("findMSB", &["value"]),
    entry("floor", &["x"]),
    entry("fma", &["a", "b", "c"]),
    entry("fract", &["x"]),
    entry("greaterThan", &["x", "y"]),
    entry("greaterThanEqual", &["x", "y"]),
    entry("imulExtended", &["x", "y", "msb", "lsb"]),
    entry("inverse", &["m"]),
    entry("inversesqrt", &["x"]),
    entry("isinf", &["x"]),
    entry("isnan", &["x"]),
    entry("length", &["x"]),
    entry("lessThan", &["x", "y"]),
    entry("lessThanEqual", &["x", "y"]),
    entry("matrixCompMult", &["x", "y"]),
    entry("max", &["x", "y"]),
    entry("min", &["x", "y"]),
    entry("mix", &["x", "y", "a"]),
    entry("mod", &["x", "y"]),
    entry("modf", &["x", "i"]),
    entry("normalize", &["v"]),
    entry("not", &["v"]),
    entry("notEqual", &["x", "y"]),
    entry("outerProduct", &["c", "r"]),
    entry("radians", &["degrees"]),
    entry("round", &["x"]),
    entry("roundEven", &["x"]),
    entry("sign", &["x"]),
    entry("smoothstep", &["edge0", "edge1", "x"]),
    entry("sqrt", &["x"]),
    entry("transpose", &["m"]),
    entry("trunc", &["x"]),
    entry("uaddCarry", &["x", "y", "carry"]),
    entry("umulExtended", &["x", "y", "msb", "lsb"]),
    entry("usubBorrow", &["x", "y", "borrow"]),
];

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::Span;
    use crate::hir::{builtin_has_out_args, builtin_kind, check_builtin_arity, is_glsl_import};

    #[test]
    fn inventory_is_sorted_and_unique() {
        let names: Vec<&str> = INLINED_BUILTINS.iter().map(|b| b.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted, "INLINED_BUILTINS must be sorted and unique");
    }

    #[test]
    fn every_entry_is_a_distinct_inlined_builtin() {
        let mut kinds = Vec::new();
        for builtin in INLINED_BUILTINS {
            let kind = builtin_kind(builtin.name).unwrap_or_else(|| {
                panic!(
                    "{:?} is not an inlined builtin (builtin_kind)",
                    builtin.name
                )
            });
            assert!(
                !kinds.contains(&kind),
                "duplicate BuiltinKind for {:?}",
                builtin.name
            );
            kinds.push(kind);
        }
        // Canary: bump alongside a new `BuiltinKind` variant + its
        // `builtin_kind` arm — the inventory entry is part of adding one.
        assert_eq!(INLINED_BUILTINS.len(), 51);
    }

    #[test]
    fn params_match_the_compilers_arity_check() {
        let span = Span::new(0, 0);
        for builtin in INLINED_BUILTINS {
            let kind = builtin_kind(builtin.name).expect("checked above");
            if builtin_has_out_args(kind) {
                // Out-arg builtins bypass `check_builtin_arity`; their
                // param lists follow the GLSL spec and are exercised by
                // the frontend's own out-arg tests.
                continue;
            }
            let arity = builtin.params.len();
            assert!(
                check_builtin_arity(span, kind, arity).is_ok(),
                "{:?}: compiler rejects arity {arity}",
                builtin.name
            );
            assert!(
                check_builtin_arity(span, kind, arity + 1).is_err(),
                "{:?}: compiler accepts arity {} (inventory too small?)",
                builtin.name,
                arity + 1
            );
        }
    }

    #[test]
    fn inventory_is_disjoint_from_runtime_imports() {
        for builtin in INLINED_BUILTINS {
            assert!(
                !is_glsl_import(builtin.name),
                "{:?} is a runtime import (BuiltinId) — it belongs to the \
                 generated mapping tables, not this inventory",
                builtin.name
            );
        }
    }
}
