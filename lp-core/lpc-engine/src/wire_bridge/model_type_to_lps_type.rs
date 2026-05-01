//! [`lpc_model::ModelType`] → [`lps_shared::LpsType`] for compiler/runtime boundaries.

use alloc::boxed::Box;
use alloc::vec::Vec;

use lpc_model::ModelType;
use lps_shared::{LpsType, StructMember};

/// Map foundation storage layout to shader ABI types.
#[must_use]
pub fn model_type_to_lps_type(ty: &ModelType) -> LpsType {
    match ty {
        ModelType::I32 => LpsType::Int,
        ModelType::U32 => LpsType::UInt,
        ModelType::F32 => LpsType::Float,
        ModelType::Bool => LpsType::Bool,
        ModelType::Vec2 => LpsType::Vec2,
        ModelType::Vec3 => LpsType::Vec3,
        ModelType::Vec4 => LpsType::Vec4,
        ModelType::IVec2 => LpsType::IVec2,
        ModelType::IVec3 => LpsType::IVec3,
        ModelType::IVec4 => LpsType::IVec4,
        ModelType::UVec2 => LpsType::UVec2,
        ModelType::UVec3 => LpsType::UVec3,
        ModelType::UVec4 => LpsType::UVec4,
        ModelType::BVec2 => LpsType::BVec2,
        ModelType::BVec3 => LpsType::BVec3,
        ModelType::BVec4 => LpsType::BVec4,
        ModelType::Mat2x2 => LpsType::Mat2,
        ModelType::Mat3x3 => LpsType::Mat3,
        ModelType::Mat4x4 => LpsType::Mat4,
        ModelType::Array(element, len) => LpsType::Array {
            element: Box::new(model_type_to_lps_type(element)),
            len: u32::try_from(*len)
                .expect("lpc-model array length must fit LpsType::Array len (u32)"),
        },
        ModelType::Struct { name, fields } => LpsType::Struct {
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
    use lpc_model::ModelStructMember;
    use lpc_model::ModelType as Mt;
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
