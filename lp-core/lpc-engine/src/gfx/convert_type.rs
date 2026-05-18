//! [`lpc_model::LpType`] → [`lps_shared::LpsType`] for compiler/runtime boundaries.

use alloc::boxed::Box;
use alloc::vec::Vec;

use lpc_model::LpType;
use lps_shared::{LpsType, StructMember};

/// Map foundation storage layout to shader ABI types.
#[must_use]
pub fn model_type_to_lps_type(ty: &LpType) -> LpsType {
    match ty {
        LpType::I32 => LpsType::Int,
        LpType::U32 => LpsType::UInt,
        LpType::F32 => LpsType::Float,
        LpType::Bool => LpsType::Bool,
        LpType::Vec2 => LpsType::Vec2,
        LpType::Vec3 => LpsType::Vec3,
        LpType::Vec4 => LpsType::Vec4,
        LpType::IVec2 => LpsType::IVec2,
        LpType::IVec3 => LpsType::IVec3,
        LpType::IVec4 => LpsType::IVec4,
        LpType::UVec2 => LpsType::UVec2,
        LpType::UVec3 => LpsType::UVec3,
        LpType::UVec4 => LpsType::UVec4,
        LpType::BVec2 => LpsType::BVec2,
        LpType::BVec3 => LpsType::BVec3,
        LpType::BVec4 => LpsType::BVec4,
        LpType::Mat2x2 => LpsType::Mat2,
        LpType::Mat3x3 => LpsType::Mat3,
        LpType::Mat4x4 => LpsType::Mat4,
        LpType::Any
        | LpType::String
        | LpType::Resource
        | LpType::Product(_)
        | LpType::Enum { .. } => {
            panic!("model type cannot be mapped to shader ABI type: {ty:?}")
        }
        LpType::Array(element, len) => LpsType::Array {
            element: Box::new(model_type_to_lps_type(element)),
            len: u32::try_from(*len)
                .expect("lpc-model array length must fit LpsType::Array len (u32)"),
        },
        LpType::List(_) => {
            unimplemented!("LpType::List does not have a shader ABI projection yet")
        }
        LpType::Struct { name, fields } => LpsType::Struct {
            name: name.clone(),
            members: fields
                .iter()
                .map(|m| StructMember {
                    name: Some(m.name.clone()),
                    ty: model_type_to_lps_type(&m.ty),
                })
                .collect::<Vec<_>>(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use lpc_model::LpType as Mt;
    use lpc_model::ModelStructMember;
    use lpc_model::kind::Kind;

    #[test]
    fn maps_primitive_storages_from_kind() {
        assert_eq!(
            model_type_to_lps_type(&Kind::Amplitude.storage()),
            LpsType::Float
        );
        assert_eq!(model_type_to_lps_type(&Kind::Count.storage()), LpsType::Int);
        assert_eq!(
            model_type_to_lps_type(&Kind::Position2d.storage()),
            LpsType::Vec2
        );
    }

    #[test]
    fn maps_texture_slot_struct_from_kind() {
        let wt = Kind::Texture.storage();
        let lt = model_type_to_lps_type(&wt);
        match lt {
            LpsType::Struct { name, members } => {
                assert_eq!(name.as_deref(), Some("Texture"));
                assert_eq!(members.len(), 4);
                assert!(members.iter().all(|m| m.ty == LpsType::Int));
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn maps_arrays() {
        let arr = Mt::Array(Box::new(Mt::F32), 8);
        match model_type_to_lps_type(&arr) {
            LpsType::Array { element, len } => {
                assert_eq!(*element, LpsType::Float);
                assert_eq!(len, 8);
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn maps_nested_struct_members() {
        let wt = Mt::Struct {
            name: Some(String::from("Outer")),
            fields: alloc::vec![ModelStructMember {
                name: String::from("x"),
                ty: Mt::F32,
            }],
        };
        match model_type_to_lps_type(&wt) {
            LpsType::Struct { members, .. } => {
                assert_eq!(members.len(), 1);
                assert_eq!(members[0].ty, LpsType::Float);
            }
            _ => panic!("expected struct"),
        }
    }
}
