use crate::{
    FieldSlot, LpType, LpValue, RelativeNodeRef, Revision, SlotDataAccess, SlotMeta, SlotShape,
    SlotShapeId, SlotValue, SlotValueAccess, SlotValueShape, ToLpValue, ValueEditorHint,
    ValueRootError, WithRevision, current_revision,
};
use alloc::string::ToString;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Revision-tracked relative node reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelativeNodeRefSlot {
    inner: WithRevision<RelativeNodeRef>,
}

impl RelativeNodeRefSlot {
    pub fn new(value: RelativeNodeRef) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(revision: Revision, value: RelativeNodeRef) -> Self {
        Self {
            inner: WithRevision::new(revision, value),
        }
    }

    pub fn set(&mut self, value: RelativeNodeRef) {
        self.inner.set(current_revision(), value);
    }

    pub fn revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &RelativeNodeRef {
        self.inner.value()
    }
}

impl SlotValueAccess for RelativeNodeRefSlot {
    fn changed_at(&self) -> Revision {
        self.inner.changed_at()
    }

    fn value(&self) -> LpValue {
        self.inner.value().to_lp_value()
    }
}

impl Serialize for RelativeNodeRefSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RelativeNodeRefSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(RelativeNodeRef::deserialize(deserializer)?))
    }
}

impl FieldSlot for RelativeNodeRefSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(relative_node_ref_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl ToLpValue for RelativeNodeRef {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.to_string())
    }
}

impl crate::FromLpValue for RelativeNodeRef {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => RelativeNodeRef::parse(&value)
                .map_err(|err| ValueRootError::new(alloc::format!("{err}"))),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for RelativeNodeRef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.relative_node_ref");

    fn value_shape() -> SlotValueShape {
        relative_node_ref_shape()
    }
}

pub fn relative_node_ref_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("slot.leaf.relative_node_ref"),
        ty: LpType::String,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::NodeRef,
    }
}
