use crate::{
    FieldSlot, FrameId, ModelType, ModelValue, OrderedF32, SlotDataAccess, SlotEditorHint,
    SlotLeafId, SlotMeta, SlotShape, SlotValueAccess, SlotValueShape, Versioned,
    current_state_version,
};

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

    fn value(&self) -> ModelValue {
        ModelValue::F32(*self.inner.value())
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
        leaf: SlotLeafId::from_static_name("slot.leaf.positive_f32"),
        ty: ModelType::F32,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Number {
            min: Some(OrderedF32(0.0)),
            max: None,
            step: None,
        },
    }
}
