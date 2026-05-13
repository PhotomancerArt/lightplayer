use crate::{
    FieldSlot, FieldSlotMut, LpType, LpValue, OrderedF32, Revision, SlotDataAccess,
    SlotDataAccessMut, SlotMeta, SlotShape, SlotShapeId, SlotValueAccess, SlotValueMut,
    SlotValueShape, ValueEditorHint, ValueRootError, WithRevision, current_revision,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// TODO: We probably want a builder pattern for slots not
//       a seperate PositiveF32Slot. More like F32Slot.positive()
//

/// Revision-tracked non-negative floating point value.
#[derive(Clone, Debug, PartialEq)]
pub struct PositiveF32Slot {
    inner: WithRevision<f32>,
}

impl PositiveF32Slot {
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

    pub fn changed_revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &f32 {
        self.inner.value()
    }
}

impl SlotValueAccess for PositiveF32Slot {
    fn changed_at(&self) -> Revision {
        self.inner.changed_at()
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

impl SlotValueMut for PositiveF32Slot {
    fn set_lp_value(&mut self, revision: Revision, value: LpValue) -> Result<(), ValueRootError> {
        let LpValue::F32(value) = value else {
            return Err(ValueRootError::new("expected f32"));
        };
        self.inner.set(revision, value);
        Ok(())
    }
}

impl FieldSlotMut for PositiveF32Slot {
    fn slot_field_data_mut(&mut self) -> SlotDataAccessMut<'_> {
        SlotDataAccessMut::Value(self)
    }
}

pub fn positive_f32_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("slot.leaf.positive_f32"),
        ty: LpType::F32,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Number {
            min: Some(OrderedF32(0.0)),
            max: None,
            step: None,
        },
    }
}
