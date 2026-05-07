use crate::{
    FieldSlot, FrameId, LpType, LpValue, OrderedF32, SlotDataAccess, ValueEditorHint,
    LpValueRootId, SlotMeta, SlotShape, SlotValueAccess, SlotValueShape, Versioned,
    current_state_version,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versioned non-negative floating point value.
#[derive(Clone, Debug, PartialEq)]
pub struct PositiveF32Slot {
    inner: Versioned<f32>,
}

impl PositiveF32Slot {
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

impl SlotValueAccess for PositiveF32Slot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> LpValue {
        LpValue::F32(*self.inner.value())
    }
}

impl Serialize for PositiveF32Slot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PositiveF32Slot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(f32::deserialize(deserializer)?))
    }
}

impl FieldSlot for PositiveF32Slot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(positive_f32_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

pub fn positive_f32_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: LpValueRootId::from_static_name("slot.leaf.positive_f32"),
        ty: LpType::F32,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Number {
            min: Some(OrderedF32(0.0)),
            max: None,
            step: None,
        },
    }
}
