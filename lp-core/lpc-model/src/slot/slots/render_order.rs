use crate::{
    FieldSlot, FrameId, LpType, LpValue, OrderedF32, SlotDataAccess, SlotEditorHint,
    SlotLeafId, SlotMeta, SlotShape, SlotValueAccess, SlotValueShape, Versioned,
    current_state_version,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versioned render ordering value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderOrderSlot {
    inner: Versioned<i32>,
}

impl RenderOrderSlot {
    pub fn new(value: i32) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: i32) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: i32) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &i32 {
        self.inner.value()
    }
}

impl SlotValueAccess for RenderOrderSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> LpValue {
        LpValue::I32(*self.inner.value())
    }
}

impl Serialize for RenderOrderSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RenderOrderSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(i32::deserialize(deserializer)?))
    }
}

impl FieldSlot for RenderOrderSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(render_order_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

pub fn render_order_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.render_order"),
        ty: LpType::I32,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Number {
            min: None,
            max: None,
            step: Some(OrderedF32(1.0)),
        },
    }
}
