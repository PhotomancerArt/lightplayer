use crate::{
    FieldSlot, Revision, FromLpValue, LpType, LpValue, ModelStructMember, SlotDataAccess, SlotMeta,
    SlotShape, SlotShapeId, SlotValue, SlotValueAccess, SlotValueShape, ToLpValue, ValueEditorHint,
    ValueRootError, WithRevision, current_revision,
};
use alloc::string::String;
use alloc::vec;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Width/height dimensions in unsigned integer pixels or cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dim2u {
    pub width: u32,
    pub height: u32,
}

/// Revision-tracked unsigned 2D dimensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dim2uSlot {
    inner: WithRevision<Dim2u>,
}

impl Dim2uSlot {
    pub fn new(value: Dim2u) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(revision: Revision, value: Dim2u) -> Self {
        Self {
            inner: WithRevision::new(revision, value),
        }
    }

    pub fn set(&mut self, value: Dim2u) {
        self.inner.set(current_revision(), value);
    }

    pub fn revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &Dim2u {
        self.inner.value()
    }
}

impl SlotValueAccess for Dim2uSlot {
    fn changed_at(&self) -> Revision {
        self.inner.changed_at()
    }

    fn value(&self) -> LpValue {
        self.inner.value().to_lp_value()
    }
}

impl Serialize for Dim2uSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Dim2uSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(Dim2u::deserialize(deserializer)?))
    }
}

impl FieldSlot for Dim2uSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(dim2u_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl ToLpValue for Dim2u {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Struct {
            name: Some(String::from("Dim2u")),
            fields: vec![
                (String::from("width"), LpValue::U32(self.width)),
                (String::from("height"), LpValue::U32(self.height)),
            ],
        }
    }
}

impl FromLpValue for Dim2u {
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { name, fields } = value else {
            return Err(ValueRootError::new("expected Dim2u struct"));
        };
        if name.as_deref() != Some("Dim2u") || fields.len() != 2 {
            return Err(ValueRootError::new("expected Dim2u struct"));
        }
        let width = match &fields[0] {
            (name, LpValue::U32(value)) if name == "width" => *value,
            _ => return Err(ValueRootError::new("expected Dim2u.width")),
        };
        let height = match &fields[1] {
            (name, LpValue::U32(value)) if name == "height" => *value,
            _ => return Err(ValueRootError::new("expected Dim2u.height")),
        };
        Ok(Self { width, height })
    }
}

impl SlotValue for Dim2u {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.dim2u");

    fn value_shape() -> SlotValueShape {
        dim2u_shape()
    }
}

pub fn dim2u_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("slot.leaf.dim2u"),
        ty: dim2u_model_type(),
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Dimensions,
    }
}

fn dim2u_model_type() -> LpType {
    LpType::Struct {
        name: Some(String::from("Dim2u")),
        fields: vec![
            ModelStructMember {
                name: String::from("width"),
                ty: LpType::U32,
            },
            ModelStructMember {
                name: String::from("height"),
                ty: LpType::U32,
            },
        ],
    }
}
