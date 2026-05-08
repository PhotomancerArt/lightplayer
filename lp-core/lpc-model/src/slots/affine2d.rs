use crate::{
    FieldSlot, Revision, FromLpValue, LpType, LpValue, ModelStructMember, SlotDataAccess, SlotMeta,
    SlotShape, SlotShapeId, SlotValue, SlotValueAccess, SlotValueShape, ToLpValue, ValueEditorHint,
    ValueRootError, WithRevision, current_revision,
};
use alloc::string::String;
use alloc::vec;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 2D affine transform with translation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

/// Revision-tracked 2D affine transform.
#[derive(Clone, Debug, PartialEq)]
pub struct Affine2dSlot {
    inner: WithRevision<Affine2d>,
}

impl Affine2dSlot {
    pub fn new(value: Affine2d) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(revision: Revision, value: Affine2d) -> Self {
        Self {
            inner: WithRevision::new(revision, value),
        }
    }

    pub fn set(&mut self, value: Affine2d) {
        self.inner.set(current_revision(), value);
    }

    pub fn revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &Affine2d {
        self.inner.value()
    }
}

impl SlotValueAccess for Affine2dSlot {
    fn changed_at(&self) -> Revision {
        self.inner.changed_at()
    }

    fn value(&self) -> LpValue {
        self.inner.value().to_lp_value()
    }
}

impl Serialize for Affine2dSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Affine2dSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(Affine2d::deserialize(deserializer)?))
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

impl ToLpValue for Affine2d {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Struct {
            name: Some(String::from("Affine2d")),
            fields: vec![
                (String::from("m00"), LpValue::F32(self.m00)),
                (String::from("m01"), LpValue::F32(self.m01)),
                (String::from("m10"), LpValue::F32(self.m10)),
                (String::from("m11"), LpValue::F32(self.m11)),
                (String::from("tx"), LpValue::F32(self.tx)),
                (String::from("ty"), LpValue::F32(self.ty)),
            ],
        }
    }
}

impl FromLpValue for Affine2d {
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { name, fields } = value else {
            return Err(ValueRootError::new("expected Affine2d struct"));
        };
        if name.as_deref() != Some("Affine2d") || fields.len() != 6 {
            return Err(ValueRootError::new("expected Affine2d struct"));
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

impl SlotValue for Affine2d {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.affine2d");

    fn value_shape() -> SlotValueShape {
        affine2d_shape()
    }
}

pub fn affine2d_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("slot.leaf.affine2d"),
        ty: affine2d_model_type(),
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Affine2d,
    }
}

fn affine2d_model_type() -> LpType {
    LpType::Struct {
        name: Some(String::from("Affine2d")),
        fields: vec![
            ModelStructMember {
                name: String::from("m00"),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: String::from("m01"),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: String::from("m10"),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: String::from("m11"),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: String::from("tx"),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: String::from("ty"),
                ty: LpType::F32,
            },
        ],
    }
}

fn struct_f32(
    fields: &[(String, LpValue)],
    index: usize,
    expected_name: &str,
) -> Result<f32, ValueRootError> {
    match fields.get(index) {
        Some((name, LpValue::F32(value))) if name == expected_name => Ok(*value),
        _ => Err(ValueRootError::new(alloc::format!(
            "expected Affine2d.{expected_name}"
        ))),
    }
}
