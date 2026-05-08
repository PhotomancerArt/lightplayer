use crate::{
    FieldSlot, Revision, LpType, LpValue, SlotDataAccess, SlotMeta, SlotShape, SlotShapeId,
    SlotValueAccess, SlotValueShape, ValueEditorHint, WithRevision, current_revision,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versioned 2D XY coordinate.
#[derive(Clone, Debug, PartialEq)]
pub struct XySlot {
    inner: WithRevision<[f32; 2]>,
}

impl XySlot {
    pub fn new(value: [f32; 2]) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(frame: Revision, value: [f32; 2]) -> Self {
        Self {
            inner: WithRevision::new(frame, value),
        }
    }

    pub fn set(&mut self, value: [f32; 2]) {
        self.inner.set(current_revision(), value);
    }

    pub fn changed_frame(&self) -> Revision {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &[f32; 2] {
        self.inner.value()
    }
}

impl SlotValueAccess for XySlot {
    fn changed_frame(&self) -> Revision {
        self.inner.changed_frame()
    }

    fn value(&self) -> LpValue {
        LpValue::Vec2(*self.inner.value())
    }
}

impl Serialize for XySlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for XySlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(<[f32; 2]>::deserialize(deserializer)?))
    }
}

impl FieldSlot for XySlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(xy_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

pub fn xy_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("slot.leaf.xy"),
        ty: LpType::Vec2,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Xy,
    }
}
