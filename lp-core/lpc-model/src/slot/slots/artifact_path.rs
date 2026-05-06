use crate::{
    FieldSlot, FrameId, ModelValue, SlotDataAccess, SlotShape, SlotValueAccess, Versioned,
    current_state_version,
};
use alloc::string::String;

use super::source_path::path_shape;

/// Versioned path to an authored artifact file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactPathSlot {
    inner: Versioned<String>,
}

impl ArtifactPathSlot {
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

impl SlotValueAccess for ArtifactPathSlot {
    fn changed_frame(&self) -> FrameId {
        self.inner.changed_frame()
    }

    fn value(&self) -> ModelValue {
        ModelValue::String(self.inner.value().clone())
    }
}

impl FieldSlot for ArtifactPathSlot {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(artifact_path_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

pub fn artifact_path_shape() -> crate::SlotValueShape {
    path_shape("slot.leaf.artifact_path")
}
