use crate::{
    FieldSlot, Revision, LpType, LpValue, OrderedF32, SlotDataAccess, SlotMeta, SlotShape,
    SlotShapeId, SlotValueAccess, SlotValueShape, ValueEditorHint, WithRevision,
    current_revision,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Revision-tracked floating point ratio in the inclusive `0.0..=1.0` domain.
#[derive(Clone, Debug, PartialEq)]
pub struct RatioSlot {
    inner: WithRevision<f32>,
}

impl RatioSlot {
    pub fn new(value: f32) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(revision: Revision, value: f32) -> Self {
        Self {
            inner: WithRevision::new(revision, value),
        }
    }

    pub fn set(&mut self, value: f32) {
        self.inner.set(current_revision(), value);
    }

    pub fn revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &f32 {
        self.inner.value()
    }
}

impl SlotValueAccess for RatioSlot {
    fn changed_at(&self) -> Revision {
        self.inner.changed_at()
    }

    fn value(&self) -> LpValue {
        LpValue::F32(*self.inner.value())
    }
}

impl Serialize for RatioSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RatioSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(f32::deserialize(deserializer)?))
    }
}

impl FieldSlot for RatioSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(ratio_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

pub fn ratio_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("slot.leaf.ratio"),
        ty: LpType::F32,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Slider {
            min: OrderedF32(0.0),
            max: OrderedF32(1.0),
            step: Some(OrderedF32(0.01)),
        },
    }
}
