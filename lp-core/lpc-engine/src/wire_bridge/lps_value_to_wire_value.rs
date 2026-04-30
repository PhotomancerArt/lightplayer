//! [`lps_shared::LpsValueF32`] → [`lpc_model::WireValue`] (lossy on texture host metadata).

use alloc::vec::Vec;

use lpc_model::WireValue;
use lps_shared::LpsValueF32;

/// Convert a runtime shader value to the portable wire/disk shape.
///
/// [`LpsValueF32::Texture2D`] maps to [`WireValue::Texture2D`] using guest
/// descriptor lanes only; [`lps_shared::LpsTexture2DValue`] host fields
/// (`format`, `byte_len`) are dropped.
#[must_use]
pub fn lps_value_f32_to_wire_value(value: &LpsValueF32) -> WireValue {
    match value {
        LpsValueF32::I32(v) => WireValue::I32(*v),
        LpsValueF32::U32(v) => WireValue::U32(*v),
        LpsValueF32::F32(v) => WireValue::F32(*v),
        LpsValueF32::Bool(v) => WireValue::Bool(*v),
        LpsValueF32::Vec2(v) => WireValue::Vec2(*v),
        LpsValueF32::Vec3(v) => WireValue::Vec3(*v),
        LpsValueF32::Vec4(v) => WireValue::Vec4(*v),
        LpsValueF32::IVec2(v) => WireValue::IVec2(*v),
        LpsValueF32::IVec3(v) => WireValue::IVec3(*v),
        LpsValueF32::IVec4(v) => WireValue::IVec4(*v),
        LpsValueF32::UVec2(v) => WireValue::UVec2(*v),
        LpsValueF32::UVec3(v) => WireValue::UVec3(*v),
        LpsValueF32::UVec4(v) => WireValue::UVec4(*v),
        LpsValueF32::BVec2(v) => WireValue::BVec2(*v),
        LpsValueF32::BVec3(v) => WireValue::BVec3(*v),
        LpsValueF32::BVec4(v) => WireValue::BVec4(*v),
        LpsValueF32::Mat2x2(v) => WireValue::Mat2x2(*v),
        LpsValueF32::Mat3x3(v) => WireValue::Mat3x3(*v),
        LpsValueF32::Mat4x4(v) => WireValue::Mat4x4(*v),
        LpsValueF32::Array(items) => WireValue::Array(
            items
                .iter()
                .map(lps_value_f32_to_wire_value)
                .collect::<Vec<_>>(),
        ),
        LpsValueF32::Struct { name, fields } => WireValue::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), lps_value_f32_to_wire_value(v)))
                .collect::<Vec<_>>(),
        },
        LpsValueF32::Texture2D(tv) => {
            let d = tv.descriptor;
            WireValue::Texture2D {
                ptr: d.ptr,
                width: d.width,
                height: d.height,
                row_stride: d.row_stride,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;
    use lps_shared::{LpsTexture2DDescriptor, LpsTexture2DValue, TextureStorageFormat};

    #[test]
    fn converts_scalar_and_vectors() {
        assert_eq!(
            lps_value_f32_to_wire_value(&LpsValueF32::F32(1.5)),
            WireValue::F32(1.5)
        );
        assert_eq!(
            lps_value_f32_to_wire_value(&LpsValueF32::Vec3([1.0, 2.0, 3.0])),
            WireValue::Vec3([1.0, 2.0, 3.0])
        );
    }

    #[test]
    fn converts_nested_array_and_struct() {
        let v = LpsValueF32::Struct {
            name: Some(String::from("S")),
            fields: vec![
                (
                    String::from("items"),
                    LpsValueF32::Array(Box::new([LpsValueF32::I32(1), LpsValueF32::I32(2)])),
                ),
                (String::from("flag"), LpsValueF32::Bool(false)),
            ],
        };
        let w = lps_value_f32_to_wire_value(&v);
        match w {
            WireValue::Struct { name, fields } => {
                assert_eq!(name.as_deref(), Some("S"));
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn texture2d_preserves_guest_descriptor_only() {
        let tv = LpsTexture2DValue {
            descriptor: LpsTexture2DDescriptor {
                ptr: 0x40,
                width: 2,
                height: 2,
                row_stride: 16,
            },
            format: TextureStorageFormat::Rgba16Unorm,
            byte_len: 256,
        };
        assert_eq!(
            lps_value_f32_to_wire_value(&LpsValueF32::Texture2D(tv)),
            WireValue::Texture2D {
                ptr: 0x40,
                width: 2,
                height: 2,
                row_stride: 16,
            }
        );
    }
}
