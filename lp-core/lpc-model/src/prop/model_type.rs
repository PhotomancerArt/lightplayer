//! Model-side structural type projection for storage and slot layout (`ModelType`).
//!
//! Model-side layout types only; conversion to shader ABI types stays in `lpc-engine`.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// Structural type for GPU-oriented storage and serializers (foundation-side).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    I32,
    U32,
    F32,
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
    Mat2x2,
    Mat3x3,
    Mat4x4,
    Array(Box<ModelType>, usize),
    Struct {
        name: Option<String>,
        fields: Vec<ModelStructMember>,
    },
}

/// One field in a [`ModelType::Struct`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ModelStructMember {
    pub name: String,
    pub ty: ModelType,
}
