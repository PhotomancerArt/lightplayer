//! GLSL type and function-signature shapes for builtin codegen (subset of the old frontend).

/// GLSL type (subset used by LPFX / builtin signature parsing).
#[allow(
    dead_code,
    reason = "mirrors full GLSL type set; not every variant appears in LPFX signatures"
)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    Bool,
    Int,
    UInt,
    Float,
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
    Sampler2D,
    Struct(StructId),
    Array(Box<Type>, usize),
    /// Placeholder when parsing fails.
    Error,
}

pub type StructId = usize;

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub return_type: Type,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    #[allow(
        dead_code,
        reason = "parameter names are preserved for future diagnostics"
    )]
    pub name: String,
    pub ty: Type,
    pub qualifier: ParamQualifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamQualifier {
    In,
    Out,
    InOut,
}
