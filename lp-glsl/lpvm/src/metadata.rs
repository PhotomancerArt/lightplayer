//! GLSL-facing metadata for JIT / WASM calls (qualifiers, logical types).
//!
//! Logical types ([`LpsType`], [`StructMember`], [`LayoutRules`]) live in
//! [`lps_types`]. This module only holds ABI metadata structs used across the
//! compiler and runtime.
//!
//! Matrix types use **column-major** component order (same as GLSL / Naga scalarization):
//! `mat2` is 4 float words `[col0row0, col0row1, col1row0, col1row1]`, etc.

use alloc::string::String;
use alloc::vec::Vec;

use lps_types::LpsType;
pub use lps_types::ParamQualifier as GlslParamQualifier;

/// One GLSL parameter after lowering (pointee type for `out` / `inout` pointers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlslParamMeta {
    pub name: String,
    pub qualifier: GlslParamQualifier,
    pub ty: LpsType,
}

/// Metadata for one user function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlslFunctionMeta {
    pub name: String,
    pub params: Vec<GlslParamMeta>,
    pub return_type: LpsType,
}

/// Full module metadata from GLSL lowering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlslModuleMeta {
    pub functions: Vec<GlslFunctionMeta>,
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn minimal_module_meta_fields() {
        let m = GlslModuleMeta {
            functions: vec![GlslFunctionMeta {
                name: String::from("add"),
                params: vec![
                    GlslParamMeta {
                        name: String::from("a"),
                        qualifier: GlslParamQualifier::In,
                        ty: LpsType::Float,
                    },
                    GlslParamMeta {
                        name: String::from("b"),
                        qualifier: GlslParamQualifier::In,
                        ty: LpsType::Float,
                    },
                ],
                return_type: LpsType::Float,
            }],
        };
        assert_eq!(m.functions[0].name, "add");
        assert_eq!(m.functions[0].params.len(), 2);
        assert_eq!(m.functions[0].return_type, LpsType::Float);
    }

    #[test]
    fn layout_rules_std430_implemented() {
        assert!(lps_types::LayoutRules::Std430.is_implemented());
        assert!(!lps_types::LayoutRules::Std140.is_implemented());
    }
}
