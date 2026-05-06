use crate::{
    FieldSlot, FrameId, ModelType, ModelValue, RelativeNodeRef, SlotDataAccess, SlotEditorHint,
    SlotLeaf, SlotLeafError, SlotLeafId, SlotMeta, SlotShape, SlotValueAccess, SlotValueShape,
    ToModelValue, Versioned, current_state_version,
};
use alloc::string::ToString;

/// Versioned relative node reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelativeNodeRefSlot {
    inner: Versioned<RelativeNodeRef>,
}

impl RelativeNodeRefSlot {
    pub fn new(value: RelativeNodeRef) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: RelativeNodeRef) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: RelativeNodeRef) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &RelativeNodeRef {
        self.inner.value()
    }
}

impl SlotValueAccess for RelativeNodeRefSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> ModelValue {
        self.inner.value().to_model_value()
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

impl ToModelValue for RelativeNodeRef {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::String(self.to_string())
    }
}

impl crate::FromModelValue for RelativeNodeRef {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        match value {
            ModelValue::String(value) => RelativeNodeRef::parse(&value)
                .map_err(|err| SlotLeafError::new(alloc::format!("{err}"))),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotLeaf for RelativeNodeRef {
    const LEAF_ID: SlotLeafId = SlotLeafId::from_static_name("slot.leaf.relative_node_ref");

    fn value_shape() -> SlotValueShape {
        relative_node_ref_shape()
    }
}

pub fn relative_node_ref_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.relative_node_ref"),
        ty: ModelType::String,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::NodeRef,
    }
}
