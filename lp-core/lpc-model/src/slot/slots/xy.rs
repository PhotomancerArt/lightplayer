use crate::{
    FieldSlot, FrameId, ModelType, ModelValue, SlotDataAccess, SlotEditorHint, SlotLeafId,
    SlotMeta, SlotShape, SlotValueAccess, SlotValueShape, Versioned, current_state_version,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versioned 2D XY coordinate.
#[derive(Clone, Debug, PartialEq)]
pub struct XySlot {
    inner: Versioned<[f32; 2]>,
}

impl XySlot {
    pub fn new(value: [f32; 2]) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: [f32; 2]) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: [f32; 2]) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &[f32; 2] {
        self.inner.value()
    }
}

impl SlotValueAccess for XySlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> ModelValue {
        ModelValue::Vec2(*self.inner.value())
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
        leaf: SlotLeafId::from_static_name("slot.leaf.xy"),
        ty: ModelType::Vec2,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Xy,
    }
}
