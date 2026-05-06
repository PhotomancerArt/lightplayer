use crate::{
    FieldSlot, FrameId, FromModelValue, ModelStructMember, ModelType, ModelValue, SlotDataAccess,
    SlotEditorHint, SlotLeaf, SlotLeafError, SlotLeafId, SlotMeta, SlotShape, SlotValueAccess,
    SlotValueShape, ToModelValue, Versioned, current_state_version,
};
use alloc::string::String;
use alloc::vec;

/// 2D affine transform with translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2d {
    pub m00: f32,
    pub m01: f32,
    pub m10: f32,
    pub m11: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Affine2d {
    pub fn identity() -> Self {
        Self {
            m00: 1.0,
            m01: 0.0,
            m10: 0.0,
            m11: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
}

/// Versioned 2D affine transform.
#[derive(Clone, Debug, PartialEq)]
pub struct Affine2dSlot {
    inner: Versioned<Affine2d>,
}

impl Affine2dSlot {
    pub fn new(value: Affine2d) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: Affine2d) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: Affine2d) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &Affine2d {
        self.inner.value()
    }
}

impl SlotValueAccess for Affine2dSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> ModelValue {
        self.inner.value().to_model_value()
    }
}

impl FieldSlot for Affine2dSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(affine2d_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl ToModelValue for Affine2d {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::Struct {
            name: Some(String::from("Affine2d")),
            fields: vec![
                (String::from("m00"), ModelValue::F32(self.m00)),
                (String::from("m01"), ModelValue::F32(self.m01)),
                (String::from("m10"), ModelValue::F32(self.m10)),
                (String::from("m11"), ModelValue::F32(self.m11)),
                (String::from("tx"), ModelValue::F32(self.tx)),
                (String::from("ty"), ModelValue::F32(self.ty)),
            ],
        }
    }
}

impl FromModelValue for Affine2d {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        let ModelValue::Struct { name, fields } = value else {
            return Err(SlotLeafError::new("expected Affine2d struct"));
        };
        if name.as_deref() != Some("Affine2d") || fields.len() != 6 {
            return Err(SlotLeafError::new("expected Affine2d struct"));
        }
        Ok(Self {
            m00: struct_f32(&fields, 0, "m00")?,
            m01: struct_f32(&fields, 1, "m01")?,
            m10: struct_f32(&fields, 2, "m10")?,
            m11: struct_f32(&fields, 3, "m11")?,
            tx: struct_f32(&fields, 4, "tx")?,
            ty: struct_f32(&fields, 5, "ty")?,
        })
    }
}

impl SlotLeaf for Affine2d {
    const LEAF_ID: SlotLeafId = SlotLeafId::from_static_name("slot.leaf.affine2d");

    fn value_shape() -> SlotValueShape {
        affine2d_shape()
    }
}

pub fn affine2d_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.affine2d"),
        ty: affine2d_model_type(),
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Affine2d,
    }
}

fn affine2d_model_type() -> ModelType {
    ModelType::Struct {
        name: Some(String::from("Affine2d")),
        fields: vec![
            ModelStructMember {
                name: String::from("m00"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("m01"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("m10"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("m11"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("tx"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("ty"),
                ty: ModelType::F32,
            },
        ],
    }
}

fn struct_f32(
    fields: &[(String, ModelValue)],
    index: usize,
    expected_name: &str,
) -> Result<f32, SlotLeafError> {
    match fields.get(index) {
        Some((name, ModelValue::F32(value))) if name == expected_name => Ok(*value),
        _ => Err(SlotLeafError::new(alloc::format!(
            "expected Affine2d.{expected_name}"
        ))),
    }
}
