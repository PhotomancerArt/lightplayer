use crate::{
    FieldSlot, FrameId, FromModelValue, ModelType, ModelValue, ResourceRef, SlotDataAccess,
    SlotEditorHint, SlotLeaf, SlotLeafError, SlotLeafId, SlotMeta, SlotShape, SlotValueAccess,
    SlotValueShape, ToModelValue, Versioned, current_state_version,
};

/// Versioned resource reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceRefSlot {
    inner: Versioned<ResourceRef>,
}

impl ResourceRefSlot {
    pub fn new(value: ResourceRef) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: ResourceRef) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: ResourceRef) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &ResourceRef {
        self.inner.value()
    }
}

impl SlotValueAccess for ResourceRefSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> ModelValue {
        self.inner.value().to_model_value()
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

impl ToModelValue for ResourceRef {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::Resource(*self)
    }
}

impl FromModelValue for ResourceRef {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        match value {
            ModelValue::Resource(value) => Ok(value),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected Resource, got {other:?}"
            ))),
        }
    }
}

impl SlotLeaf for ResourceRef {
    const LEAF_ID: SlotLeafId = SlotLeafId::from_static_name("slot.leaf.resource_ref");

    fn value_shape() -> SlotValueShape {
        resource_ref_shape()
    }
}

pub fn resource_ref_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.resource_ref"),
        ty: ModelType::Resource,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Resource,
    }
}

pub fn runtime_buffer_resource_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.runtime_buffer_resource"),
        ty: ModelType::Resource,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::RuntimeBufferResource,
    }
}

pub fn render_product_resource_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.render_product_resource"),
        ty: ModelType::Resource,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::RenderProductResource,
    }
}
