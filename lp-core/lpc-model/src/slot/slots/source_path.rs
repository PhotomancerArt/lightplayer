use crate::{
    FieldSlot, FrameId, ModelType, ModelValue, SlotDataAccess, SlotEditorHint, SlotLeafId,
    SlotMeta, SlotShape, SlotValueAccess, SlotValueShape, Versioned, current_state_version,
};
use alloc::string::String;

/// Versioned path to an authored source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePathSlot {
    inner: Versioned<String>,
}

impl SourcePathSlot {
    pub fn new(value: String) -> Self {
        Self::with_version(current_state_version(), value)
    }

    pub fn with_version(frame: FrameId, value: String) -> Self {
        Self {
            inner: Versioned::new(frame, value),
        }
    }

    pub fn set(&mut self, value: String) {
        self.inner.set(current_state_version(), value);
    }

    pub fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    pub fn value(&self) -> &String {
        self.inner.value()
    }
}

impl SlotValueAccess for SourcePathSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> ModelValue {
        ModelValue::String(self.inner.value().clone())
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

pub fn source_path_shape() -> SlotValueShape {
    path_shape("slot.leaf.source_path")
}

pub(super) fn path_shape(name: &str) -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name(name),
        ty: ModelType::String,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Path,
    }
}
