//! [`lpc_model::WireType`] → [`lps_shared::LpsType`] for compiler/runtime boundaries.

use alloc::boxed::Box;
use alloc::vec::Vec;

use lpc_model::WireType;
use lps_shared::{LpsType, StructMember};

/// Map foundation storage layout to shader ABI types.
#[must_use]
pub fn wire_type_to_lps_type(ty: &WireType) -> LpsType {
    match ty {
        WireType::I32 => LpsType::Int,
        WireType::U32 => LpsType::UInt,
        WireType::F32 => LpsType::Float,
        WireType::Bool => LpsType::Bool,
        WireType::Vec2 => LpsType::Vec2,
        WireType::Vec3 => LpsType::Vec3,
        WireType::Vec4 => LpsType::Vec4,
        WireType::IVec2 => LpsType::IVec2,
        WireType::IVec3 => LpsType::IVec3,
        WireType::IVec4 => LpsType::IVec4,
        WireType::UVec2 => LpsType::UVec2,
        WireType::UVec3 => LpsType::UVec3,
        WireType::UVec4 => LpsType::UVec4,
        WireType::BVec2 => LpsType::BVec2,
        WireType::BVec3 => LpsType::BVec3,
        WireType::BVec4 => LpsType::BVec4,
        WireType::Mat2x2 => LpsType::Mat2,
        WireType::Mat3x3 => LpsType::Mat3,
        WireType::Mat4x4 => LpsType::Mat4,
        WireType::Texture2D => LpsType::Texture2D,
        WireType::Array(element, len) => LpsType::Array {
            element: Box::new(wire_type_to_lps_type(element)),
            len: u32::try_from(*len)
                .expect("lpc-model wire array length must fit LpsType::Array len (u32)"),
        },
        WireType::Struct { name, fields } => LpsType::Struct {
            name: name.clone(),
            members: fields
                .iter()
                .map(|m| StructMember {
                    name: Some(m.name.clone()),
                    ty: wire_type_to_lps_type(&m.ty),
                })
                .collect::<Vec<_>>(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use lpc_model::WireStructMember;
    use lpc_model::WireType as Wt;
    use lpc_model::kind::Kind;

    #[test]
    fn maps_primitive_storages_from_kind() {
        assert_eq!(
            wire_type_to_lps_type(&Kind::Amplitude.storage()),
            LpsType::Float
        );
        assert_eq!(wire_type_to_lps_type(&Kind::Count.storage()), LpsType::Int);
        assert_eq!(
            wire_type_to_lps_type(&Kind::Position2d.storage()),
            LpsType::Vec2
        );
    }

    #[test]
    fn maps_texture_slot_struct_from_kind() {
        let wt = Kind::Texture.storage();
        let lt = wire_type_to_lps_type(&wt);
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
    fn maps_gpu_texture2d_and_arrays() {
        assert_eq!(wire_type_to_lps_type(&Wt::Texture2D), LpsType::Texture2D);
        let arr = Wt::Array(Box::new(Wt::F32), 8);
        match wire_type_to_lps_type(&arr) {
            LpsType::Array { element, len } => {
                assert_eq!(*element, LpsType::Float);
                assert_eq!(len, 8);
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn maps_nested_struct_members() {
        let wt = Wt::Struct {
            name: Some(String::from("Outer")),
            fields: alloc::vec![WireStructMember {
                name: String::from("x"),
                ty: Wt::F32,
            }],
        };
        match wire_type_to_lps_type(&wt) {
            LpsType::Struct { members, .. } => {
                assert_eq!(members.len(), 1);
                assert_eq!(members[0].ty, LpsType::Float);
            }
            _ => panic!("expected struct"),
        }
    }
}
