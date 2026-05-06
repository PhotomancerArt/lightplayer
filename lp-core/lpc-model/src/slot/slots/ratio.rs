use crate::{
    FieldSlot, FrameId, ModelType, ModelValue, OrderedF32, SlotDataAccess, SlotEditorHint,
    SlotLeafId, SlotMeta, SlotShape, SlotValueAccess, SlotValueShape, Versioned,
    current_state_version,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versioned floating point ratio in the inclusive `0.0..=1.0` domain.
#[derive(Clone, Debug, PartialEq)]
pub struct RatioSlot {
    inner: Versioned<f32>,
}

impl RatioSlot {
    pub fn new(value: f32) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: f32) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: f32) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &f32 {
        self.inner.value()
    }
}

impl SlotValueAccess for RatioSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> ModelValue {
        ModelValue::F32(*self.inner.value())
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
        leaf: SlotLeafId::from_static_name("slot.leaf.ratio"),
        ty: ModelType::F32,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Slider {
            min: OrderedF32(0.0),
            max: OrderedF32(1.0),
            step: Some(OrderedF32(0.01)),
        },
    }
}
