use crate::project::FrameId;
use crate::state::StateField;
use alloc::string::String;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::impl_state_serialization;

/// Shader node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderState {
    /// Actual GLSL code loaded from file
    pub glsl_code: StateField<String>,
    /// Compilation/runtime errors
    pub error: StateField<Option<String>>,
}

impl ShaderState {
    /// Create a new ShaderState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            glsl_code: StateField::new(frame_id, String::new()),
            error: StateField::new(frame_id, None),
        }
    }

    /// Merge fields from a partial update into this state
    ///
    /// Only fields that are present in `other` (non-default values) are merged.
    /// Fields not present in the partial update are preserved from `self`.
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        // Merge glsl_code if present (not empty)
        if !other.glsl_code.value().is_empty() {
            self.glsl_code
                .set(frame_id, other.glsl_code.value().clone());
        }
        // Merge error if present (Some value)
        if other.error.value().is_some() {
            self.error.set(frame_id, other.error.value().clone());
        }
    }
}

impl_state_serialization! {
    ShaderState => SerializableShaderState {
        glsl_code: String,
        error: Option<String>,
    }
}
