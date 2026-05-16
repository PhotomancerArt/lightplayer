//! Portable value type grammar for [`LpValue`](crate::LpValue).
//!
//! `LpType` validates the structural storage form of values that cross disk and
//! wire boundaries. It intentionally carries no labels, editor hints, or domain
//! semantics; those live on slot value shapes. Conversion to shader ABI types
//! stays in `lpc-engine`.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::ProductKind;

/// Structural storage type for portable LightPlayer values.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LpType {
    /// Dynamically typed payload. This is mainly for definition-time values such
    /// as literal binding endpoints where the surrounding context supplies
    /// semantic constraints later.
    Any,
    String,
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
    /// Fixed-size homogeneous sequence.
    Array(Box<LpType>, usize),
    /// Variable-length homogeneous sequence.
    List(Box<LpType>),
    Struct {
        name: Option<String>,
        fields: Vec<ModelStructMember>,
    },
    Enum {
        name: Option<String>,
        variants: Vec<ModelEnumVariant>,
    },
    Resource,
    Product(ProductKind),
}

/// One field in a [`LpType::Struct`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ModelStructMember {
    pub name: String,
    pub ty: LpType,
}

/// One variant in an [`LpType::Enum`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ModelEnumVariant {
    pub name: String,
    pub payload: Option<LpType>,
}

#[cfg(test)]
mod tests {
    use super::{LpType, ModelEnumVariant};
    use alloc::boxed::Box;

    #[test]
    fn lp_type_resource_round_trips() {
        let ty = LpType::Resource;
        let json = serde_json::to_string(&ty).unwrap();
        let back: LpType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ty);
    }

    #[test]
    fn lp_type_list_round_trips() {
        let ty = LpType::List(Box::new(LpType::U32));
        let json = serde_json::to_string(&ty).unwrap();
        let back: LpType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ty);
    }

    #[test]
    fn lp_type_enum_round_trips() {
        let ty = LpType::Enum {
            name: Some(alloc::string::String::from("BindingEndpoint")),
            variants: alloc::vec![
                ModelEnumVariant {
                    name: alloc::string::String::from("Unset"),
                    payload: None,
                },
                ModelEnumVariant {
                    name: alloc::string::String::from("Literal"),
                    payload: Some(LpType::Any),
                },
            ],
        };
        let json = serde_json::to_string(&ty).unwrap();
        let back: LpType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ty);
    }
}
