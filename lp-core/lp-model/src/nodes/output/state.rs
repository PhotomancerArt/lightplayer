use crate::project::FrameId;
use crate::state::StateField;
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

/// Output node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputState {
    /// Channel data buffer
    pub channel_data: StateField<Vec<u8>>,
}

impl OutputState {
    /// Create a new OutputState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            channel_data: StateField::new(frame_id, Vec::new()),
        }
    }
}

/// Wrapper for serializing OutputState with a since_frame context
pub struct SerializableOutputState<'a> {
    state: &'a OutputState,
    since_frame: FrameId,
}

impl<'a> SerializableOutputState<'a> {
    pub fn new(state: &'a OutputState, since_frame: FrameId) -> Self {
        Self { state, since_frame }
    }
}

impl<'a> Serialize for SerializableOutputState<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let is_initial_sync = self.since_frame == FrameId::default();
        let mut state = serializer.serialize_struct("OutputState", 1)?;

        if is_initial_sync || self.state.channel_data.changed_frame() > self.since_frame {
            // Serialize channel_data as base64 string
            use base64::Engine;
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(self.state.channel_data.value());
            state.serialize_field("channel_data", &encoded)?;
        }

        state.end()
    }
}

// Temporary: Simple Serialize implementation for NodeState compatibility
impl Serialize for OutputState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use base64::Engine;
        let mut state = serializer.serialize_struct("OutputState", 1)?;
        // Serialize channel_data as base64 string
        let encoded = base64::engine::general_purpose::STANDARD.encode(self.channel_data.value());
        state.serialize_field("channel_data", &encoded)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for OutputState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct OutputStateHelper {
            channel_data: Option<String>, // Base64 encoded string
        }

        let helper = OutputStateHelper::deserialize(deserializer)?;

        let frame_id = FrameId::default();
        let mut state = OutputState::new(frame_id);

        if let Some(encoded) = helper.channel_data {
            // Decode base64 string to Vec<u8>
            use base64::Engine;
            match base64::engine::general_purpose::STANDARD.decode(&encoded) {
                Ok(decoded) => {
                    state.channel_data.set(frame_id, decoded);
                }
                Err(_) => {
                    // Invalid base64, leave as default
                }
            }
        }

        Ok(state)
    }
}
