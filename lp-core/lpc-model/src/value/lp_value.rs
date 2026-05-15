//! Portable structural value payloads.
//!
//! `LpValue` is the disk and wire representation used at slot value leaves. It
//! may contain internal value structure, but that structure is opaque to the
//! slot tree: the whole payload is versioned, watched, patched, and mutated as
//! one logical value.

use crate::{ProductRef, ResourceRef};
use alloc::string::String;
use alloc::vec::Vec;

/// Value form crossing disk and wire boundaries.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum LpValue {
    String(String),
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
    /// Sequence payload used for both fixed [`LpType::Array`](crate::LpType::Array)
    /// and variable-length [`LpType::List`](crate::LpType::List) storage.
    Array(Vec<LpValue>),
    Struct {
        name: Option<String>,
        fields: Vec<(String, LpValue)>,
    },
    /// Store-backed materialized payload.
    Resource(ResourceRef),
    /// Lazy node-owned graph product.
    Product(ProductRef),
}

impl crate::SlotCodec for LpValue {
    fn read_slot<S>(
        value: crate::slot_codec::ValueReader<'_, '_, S>,
    ) -> Result<Self, crate::slot_codec::SyntaxError>
    where
        S: crate::slot_codec::SyntaxEventSource,
    {
        value.lp_value()
    }

    fn write_slot<W>(
        &self,
        value: crate::slot_codec::SlotValueWriter<'_, W>,
    ) -> Result<(), crate::slot_codec::SlotWriteError<W::Error>>
    where
        W: crate::slot_codec::SlotWrite,
    {
        crate::slot_codec::write_untyped_lp_value(value, self)
    }
}

#[cfg(test)]
mod tests {
    use super::LpValue;
    use alloc::string::String;
    use alloc::vec;

    #[test]
    fn lp_value_serde_roundtrip_scalar_and_vectors() {
        for v in [
            LpValue::I32(-1),
            LpValue::F32(1.5),
            LpValue::Bool(true),
            LpValue::Vec2([0.0, 1.0]),
            LpValue::Vec3([1.0, 2.0, 3.0]),
            LpValue::Resource(crate::ResourceRef::runtime_buffer(
                crate::RuntimeBufferId::new(9),
            )),
            LpValue::Product(crate::ProductRef::visual(crate::VisualProduct::new(
                crate::NodeId::new(2),
                0,
            ))),
            LpValue::Product(crate::ProductRef::control(crate::ControlProduct::new(
                crate::NodeId::new(3),
                0,
                crate::ControlExtent::new(1, 12),
            ))),
        ] {
            let json = serde_json::to_string(&v).unwrap();
            let back: LpValue = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn lp_value_serde_roundtrip_array_and_struct() {
        let v = LpValue::Struct {
            name: Some(String::from("S")),
            fields: vec![
                (
                    String::from("items"),
                    LpValue::Array(vec![LpValue::I32(1), LpValue::I32(2)]),
                ),
                (String::from("flag"), LpValue::Bool(false)),
            ],
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: LpValue = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }
}
