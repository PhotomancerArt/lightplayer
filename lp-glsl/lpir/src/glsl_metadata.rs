//! GLSL-facing metadata for JIT / WASM calls (qualifiers, logical types).
//!
//! Matrix types use **column-major** component order (same as GLSL / Naga scalarization):
//! `mat2` is 4 float words `[col0row0, col0row1, col1row0, col1row1]`, etc.

use alloc::string::String;
use alloc::vec::Vec;

/// GLSL parameter direction for Level-1 marshalling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlslParamQualifier {
    In,
    Out,
    InOut,
}

/// Logical GLSL type (scalar, vector, or square matrix) for one parameter or return.
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
}

/// One GLSL parameter after lowering (pointee type for `out` / `inout` pointers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlslParamMeta {
    pub name: String,
    pub qualifier: GlslParamQualifier,
    pub ty: GlslType,
}

/// Metadata for one user function, aligned with [`crate::IrModule::functions`] order.
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
}
