use crate::{
    FieldSlot, LpType, LpValue, OrderedF32, Revision, SlotDataAccess, SlotMeta, SlotShape,
    SlotShapeId, SlotValueAccess, SlotValueShape, ValueEditorHint, WithRevision, current_revision,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Revision-tracked render ordering value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderOrderSlot {
    inner: WithRevision<i32>,
}

impl RenderOrderSlot {
    pub fn new(value: i32) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(revision: Revision, value: i32) -> Self {
        Self {
            inner: WithRevision::new(revision, value),
        }
    }

    pub fn set(&mut self, value: i32) {
        self.inner.set(current_revision(), value);
    }

    pub fn revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &i32 {
        self.inner.value()
    }
}

impl SlotValueAccess for RenderOrderSlot {
    fn changed_at(&self) -> Revision {
        self.inner.changed_at()
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
        id: SlotShapeId::from_static_name("slot.leaf.render_order"),
        ty: LpType::I32,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Number {
            min: None,
            max: None,
            step: Some(OrderedF32(1.0)),
        },
    }
}
