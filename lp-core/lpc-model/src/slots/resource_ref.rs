use crate::{
    FieldSlot, FromLpValue, LpType, LpValue, ResourceRef, Revision, SlotDataAccess, SlotMeta,
    SlotShape, SlotShapeId, SlotValue, SlotValueAccess, SlotValueShape, ToLpValue, ValueEditorHint,
    ValueRootError, WithRevision, current_revision,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Revision-tracked resource reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceRefSlot {
    inner: WithRevision<ResourceRef>,
}

impl ResourceRefSlot {
    pub fn new(value: ResourceRef) -> Self {
        Self::with_version(current_revision(), value)
    }

    pub fn with_version(revision: Revision, value: ResourceRef) -> Self {
        Self {
            inner: WithRevision::new(revision, value),
        }
    }

    pub fn set(&mut self, value: ResourceRef) {
        self.inner.set(current_revision(), value);
    }

    pub fn revision(&self) -> Revision {
        self.inner.changed_at()
    }

    pub fn value(&self) -> &ResourceRef {
        self.inner.value()
    }
}

impl SlotValueAccess for ResourceRefSlot {
    fn changed_at(&self) -> Revision {
        self.inner.changed_at()
    }

    fn value(&self) -> LpValue {
        self.inner.value().to_lp_value()
    }
}

impl Serialize for ResourceRefSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ResourceRefSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(ResourceRef::deserialize(deserializer)?))
    }
}

impl FieldSlot for ResourceRefSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(resource_ref_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl ToLpValue for ResourceRef {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Resource(*self)
    }
}

impl FromLpValue for ResourceRef {
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::Resource(value) => Ok(value),
            other => Err(ValueRootError::new(alloc::format!(
                "expected Resource, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ResourceRef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.resource_ref");

    fn value_shape() -> SlotValueShape {
        resource_ref_shape()
    }
}

pub fn resource_ref_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("slot.leaf.resource_ref"),
        ty: LpType::Resource,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Resource,
    }
}

pub fn runtime_buffer_resource_shape() -> SlotValueShape {
    SlotValueShape {
        id: SlotShapeId::from_static_name("slot.leaf.runtime_buffer_resource"),
        ty: LpType::Resource,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::RuntimeBufferResource,
    }
}
