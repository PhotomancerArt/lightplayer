//! Portable structural value shape (`ModelValue`), serde-friendly at the foundation layer.

use alloc::string::String;
use alloc::vec::Vec;

/// Value form crossing disk/wire boundaries; serde-friendly at the foundation layer (historical
/// sibling of the wire mirror inside `lpc_source` value specs).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelValue {
    I32(i32),
    U32(u32),
    F32(f32),
    Bool(bool),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[f32; 2]; 2]),
    Mat3x3([[f32; 3]; 3]),
    Mat4x4([[f32; 4]; 4]),
    /// Texture buffer layout on the wire (width, height, row stride; `ptr` is an opaque id / handle).
    Texture2D {
        ptr: u32,
        width: u32,
        height: u32,
        row_stride: u32,
    },
    Array(Vec<ModelValue>),
    Struct {
        name: Option<String>,
        fields: Vec<(String, ModelValue)>,
    },
}

#[cfg(test)]
mod tests {
    use super::ModelValue;
    use alloc::string::String;
    use alloc::vec;

    #[test]
    fn model_value_serde_roundtrip_scalar_and_vectors() {
        for v in [
            ModelValue::I32(-1),
            ModelValue::F32(1.5),
            ModelValue::Bool(true),
            ModelValue::Vec2([0.0, 1.0]),
            ModelValue::Vec3([1.0, 2.0, 3.0]),
        ] {
            let json = serde_json::to_string(&v).unwrap();
            let back: ModelValue = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn model_value_serde_roundtrip_texture2d_descriptor() {
        let v = ModelValue::Texture2D {
            ptr: 0x1000,
            width: 64,
            height: 32,
            row_stride: 256,
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: ModelValue = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn model_value_serde_roundtrip_array_and_struct() {
        let v = ModelValue::Struct {
            name: Some(String::from("S")),
            fields: vec![
                (
                    String::from("items"),
                    ModelValue::Array(vec![ModelValue::I32(1), ModelValue::I32(2)]),
                ),
                (String::from("flag"), ModelValue::Bool(false)),
            ],
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: ModelValue = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }
}
