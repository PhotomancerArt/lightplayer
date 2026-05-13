use crate::{
    FieldSlot, FieldSlotMut, LpType, LpValue, Revision, SlotDataAccess, SlotDataAccessMut,
    SlotMeta, SlotShape, SlotShapeId, SlotValueAccess, SlotValueMut, SlotValueShape,
    ValueEditorHint, ValueRootError, WithRevision, current_revision,
};
use alloc::string::String;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Revision-tracked path to an authored source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePathSlot {
    inner: WithRevision<String>,
}

impl SourcePathSlot {
    pub fn new(value: String) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(revision: Revision, value: String) -> Self {
        Self {
            inner: WithRevision::new(revision, value),
        }
    }

    pub fn set(&mut self, value: String) {
        self.inner.set(current_revision(), value);
    }

    pub fn revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &String {
        self.inner.value()
    }
}

impl SlotValueAccess for SourcePathSlot {
    fn changed_at(&self) -> Revision {
        self.inner.changed_at()
    }

    fn value(&self) -> LpValue {
        LpValue::String(self.inner.value().clone())
    }
}

impl Serialize for SourcePathSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SourcePathSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(String::deserialize(deserializer)?))
    }
}

impl FieldSlot for SourcePathSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(source_path_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl SlotValueMut for SourcePathSlot {
    fn set_lp_value(&mut self, revision: Revision, value: LpValue) -> Result<(), ValueRootError> {
        let LpValue::String(value) = value else {
            return Err(ValueRootError::new("expected String"));
        };
        self.inner.set(revision, value);
        Ok(())
    }
}

impl FieldSlotMut for SourcePathSlot {
    fn slot_field_data_mut(&mut self) -> SlotDataAccessMut<'_> {
        SlotDataAccessMut::Value(self)
    }
}

pub fn source_path_shape() -> SlotValueShape {
    path_shape("slot.leaf.source_path")
}

pub(crate) fn path_shape(name: &str) -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name(name),
        ty: LpType::String,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Path,
    }
}
