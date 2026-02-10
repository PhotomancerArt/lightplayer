use crate::project::FrameId;
use crate::state::StateField;
use alloc::string::String;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

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
}

/// Wrapper for serializing ShaderState with a since_frame context
pub struct SerializableShaderState<'a> {
    state: &'a ShaderState,
    since_frame: FrameId,
}

impl<'a> SerializableShaderState<'a> {
    pub fn new(state: &'a ShaderState, since_frame: FrameId) -> Self {
        Self { state, since_frame }
    }
}

impl<'a> Serialize for SerializableShaderState<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let is_initial_sync = self.since_frame == FrameId::default();
        let mut state = serializer.serialize_struct("ShaderState", 2)?;

        if is_initial_sync || self.state.glsl_code.changed_frame() > self.since_frame {
            state.serialize_field("glsl_code", self.state.glsl_code.value())?;
        }
        if is_initial_sync || self.state.error.changed_frame() > self.since_frame {
            state.serialize_field("error", self.state.error.value())?;
        }

        state.end()
    }
}

// Temporary: Simple Serialize implementation for NodeState compatibility
impl Serialize for ShaderState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ShaderState", 2)?;
        state.serialize_field("glsl_code", self.glsl_code.value())?;
        state.serialize_field("error", self.error.value())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ShaderState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ShaderStateHelper {
            glsl_code: Option<String>,
            error: Option<Option<String>>,
        }

        let helper = ShaderStateHelper::deserialize(deserializer)?;

        let frame_id = FrameId::default();
        let mut state = ShaderState::new(frame_id);

        if let Some(val) = helper.glsl_code {
            state.glsl_code.set(frame_id, val);
        }
        if let Some(val) = helper.error {
            state.error.set(frame_id, val);
        }

        Ok(state)
    }
}
