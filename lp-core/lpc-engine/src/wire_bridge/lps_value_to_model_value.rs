//! [`lps_shared::LpsValueF32`] → [`lpc_model::LpValue`] for portable scalar/struct shapes.
//!
//! [`LpsValueF32::Texture2D`] is runtime/shader ABI state and is not represented as [`LpValue`].

use alloc::vec::Vec;

use lpc_model::LpValue;
use lps_shared::LpsValueF32;

/// Failure converting a runtime shader value to the portable model/disk shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LpsValueToModelConversionError {
    /// [`LpsValueF32::Texture2D`] has no portable [`LpValue`] mapping (textures are source/engine domain).
    Texture2dNotPortable,
}

/// Convert a runtime shader value to the portable model/disk shape.
///
/// Returns [`LpsValueToModelConversionError::Texture2dNotPortable`] for
/// [`LpsValueF32::Texture2D`] (and for compound values that contain one).
#[must_use]
pub fn lps_value_f32_to_model_value(
    value: &LpsValueF32,
) -> Result<LpValue, LpsValueToModelConversionError> {
    match value {
        LpsValueF32::I32(v) => Ok(LpValue::I32(*v)),
        LpsValueF32::U32(v) => Ok(LpValue::U32(*v)),
        LpsValueF32::F32(v) => Ok(LpValue::F32(*v)),
        LpsValueF32::Bool(v) => Ok(LpValue::Bool(*v)),
        LpsValueF32::Vec2(v) => Ok(LpValue::Vec2(*v)),
        LpsValueF32::Vec3(v) => Ok(LpValue::Vec3(*v)),
        LpsValueF32::Vec4(v) => Ok(LpValue::Vec4(*v)),
        LpsValueF32::IVec2(v) => Ok(LpValue::IVec2(*v)),
        LpsValueF32::IVec3(v) => Ok(LpValue::IVec3(*v)),
        LpsValueF32::IVec4(v) => Ok(LpValue::IVec4(*v)),
        LpsValueF32::UVec2(v) => Ok(LpValue::UVec2(*v)),
        LpsValueF32::UVec3(v) => Ok(LpValue::UVec3(*v)),
        LpsValueF32::UVec4(v) => Ok(LpValue::UVec4(*v)),
        LpsValueF32::BVec2(v) => Ok(LpValue::BVec2(*v)),
        LpsValueF32::BVec3(v) => Ok(LpValue::BVec3(*v)),
        LpsValueF32::BVec4(v) => Ok(LpValue::BVec4(*v)),
        LpsValueF32::Mat2x2(v) => Ok(LpValue::Mat2x2(*v)),
        LpsValueF32::Mat3x3(v) => Ok(LpValue::Mat3x3(*v)),
        LpsValueF32::Mat4x4(v) => Ok(LpValue::Mat4x4(*v)),
        LpsValueF32::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items.iter() {
                out.push(lps_value_f32_to_model_value(item)?);
            }
            Ok(LpValue::Array(out))
        }
        LpsValueF32::Struct { name, fields } => {
            let mut out_fields = Vec::with_capacity(fields.len());
            for (k, v) in fields.iter() {
                out_fields.push((k.clone(), lps_value_f32_to_model_value(v)?));
            }
            Ok(LpValue::Struct {
                name: name.clone(),
                fields: out_fields,
            })
        }
        LpsValueF32::Texture2D(_) => Err(LpsValueToModelConversionError::Texture2dNotPortable),
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
            lps_value_f32_to_model_value(&LpsValueF32::F32(1.5)),
            Ok(LpValue::F32(1.5))
        );
        assert_eq!(
            lps_value_f32_to_model_value(&LpsValueF32::Vec3([1.0, 2.0, 3.0])),
            Ok(LpValue::Vec3([1.0, 2.0, 3.0]))
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
        let w = lps_value_f32_to_model_value(&v).unwrap();
        match w {
            LpValue::Struct { name, fields } => {
                assert_eq!(name.as_deref(), Some("S"));
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn texture2d_is_not_portable_to_model_value() {
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
            lps_value_f32_to_model_value(&LpsValueF32::Texture2D(tv)),
            Err(LpsValueToModelConversionError::Texture2dNotPortable)
        );
    }
}
