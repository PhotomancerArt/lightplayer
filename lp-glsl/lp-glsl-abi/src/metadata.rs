//! GLSL-facing metadata for JIT / WASM calls (qualifiers, logical types).
//!
//! Matrix types use **column-major** component order (same as GLSL / Naga scalarization):
//! `mat2` is 4 float words `[col0row0, col0row1, col1row0, col1row1]`, etc.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// Memory layout rules for structured/uniform-like data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutRules {
    /// `std430` — storage-buffer style packing (default for LightPlayer).
    Std430,
    /// Reserved; not implemented yet.
    Std140,
}

impl LayoutRules {
    pub fn is_implemented(self) -> bool {
        matches!(self, LayoutRules::Std430)
    }
}

/// One field in a [`GlslType::Struct`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructMember {
    pub name: Option<String>,
    pub ty: GlslType,
}

/// GLSL parameter direction for Level-1 marshalling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlslParamQualifier {
    In,
    Out,
    InOut,
}

/// Logical GLSL type (scalar, vector, square matrix, array, struct) for parameters, returns, and layouts.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GlslType {
    Void,
    Float,
    Int,
    UInt,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    BVec2,
    BVec3,
    BVec4,
    Mat2,
    Mat3,
    Mat4,
    /// Fixed-size array `T[n]`; ABI is `n` flattened scalars (row-major).
    Array {
        element: Box<GlslType>,
        len: u32,
    },
    /// Struct type (layout follows active [`LayoutRules`], default `std430`).
    Struct {
        name: Option<String>,
        members: Vec<StructMember>,
    },
}

impl GlslType {
    /// Byte size under `rules`.
    pub fn size(&self, rules: LayoutRules) -> usize {
        crate::layout::type_size(self, rules)
    }

    /// Alignment under `rules`.
    pub fn alignment(&self, rules: LayoutRules) -> usize {
        crate::layout::type_alignment(self, rules)
    }

    pub fn is_scalar(&self) -> bool {
        matches!(
            self,
            GlslType::Float | GlslType::Int | GlslType::UInt | GlslType::Bool
        )
    }

    pub fn is_aggregate(&self) -> bool {
        matches!(self, GlslType::Array { .. } | GlslType::Struct { .. })
    }
}

/// One GLSL parameter after lowering (pointee type for `out` / `inout` pointers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlslParamMeta {
    pub name: String,
    pub qualifier: GlslParamQualifier,
    pub ty: GlslType,
}

/// Metadata for one user function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlslFunctionMeta {
    pub name: String,
    pub params: Vec<GlslParamMeta>,
    pub return_type: GlslType,
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
                        ty: GlslType::Float,
                    },
                    GlslParamMeta {
                        name: String::from("b"),
                        qualifier: GlslParamQualifier::In,
                        ty: GlslType::Float,
                    },
                ],
                return_type: GlslType::Float,
            }],
        };
        assert_eq!(m.functions[0].name, "add");
        assert_eq!(m.functions[0].params.len(), 2);
        assert_eq!(m.functions[0].return_type, GlslType::Float);
    }

    #[test]
    fn layout_rules_std430_implemented() {
        assert!(LayoutRules::Std430.is_implemented());
        assert!(!LayoutRules::Std140.is_implemented());
    }
}
