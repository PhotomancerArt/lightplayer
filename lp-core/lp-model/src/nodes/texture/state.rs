use crate::project::FrameId;
use crate::state::StateField;
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

/// Texture node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureState {
    /// Texture pixel data
    pub texture_data: StateField<Vec<u8>>,
    /// Texture width in pixels
    pub width: StateField<u32>,
    /// Texture height in pixels
    pub height: StateField<u32>,
    /// Texture format (e.g., "RGB8", "RGBA8", "R8")
    pub format: StateField<String>,
}

impl TextureState {
    /// Create a new TextureState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            texture_data: StateField::new(frame_id, Vec::new()),
            width: StateField::new(frame_id, 0),
            height: StateField::new(frame_id, 0),
            format: StateField::new(frame_id, String::from("RGBA8")),
        }
    }
}

/// Wrapper for serializing TextureState with a since_frame context
pub struct SerializableTextureState<'a> {
    state: &'a TextureState,
    since_frame: FrameId,
}

impl<'a> SerializableTextureState<'a> {
    pub fn new(state: &'a TextureState, since_frame: FrameId) -> Self {
        Self { state, since_frame }
    }
}

impl<'a> Serialize for SerializableTextureState<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let is_initial_sync = self.since_frame == FrameId::default();
        let mut state = serializer.serialize_struct("TextureState", 4)?;

        if is_initial_sync || self.state.texture_data.changed_frame() > self.since_frame {
            // Serialize texture_data as base64 string
            use base64::Engine;
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(self.state.texture_data.value());
            state.serialize_field("texture_data", &encoded)?;
        }
        if is_initial_sync || self.state.width.changed_frame() > self.since_frame {
            state.serialize_field("width", self.state.width.value())?;
        }
        if is_initial_sync || self.state.height.changed_frame() > self.since_frame {
            state.serialize_field("height", self.state.height.value())?;
        }
        if is_initial_sync || self.state.format.changed_frame() > self.since_frame {
            state.serialize_field("format", self.state.format.value())?;
        }

        state.end()
    }
}

// Temporary: Simple Serialize implementation for NodeState compatibility
impl Serialize for TextureState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use base64::Engine;
        let mut state = serializer.serialize_struct("TextureState", 4)?;
        // Serialize texture_data as base64 string
        let encoded = base64::engine::general_purpose::STANDARD.encode(self.texture_data.value());
        state.serialize_field("texture_data", &encoded)?;
        state.serialize_field("width", self.width.value())?;
        state.serialize_field("height", self.height.value())?;
        state.serialize_field("format", self.format.value())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for TextureState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct TextureStateHelper {
            texture_data: Option<String>, // Base64 encoded string
            width: Option<u32>,
            height: Option<u32>,
            format: Option<String>,
        }

        let helper = TextureStateHelper::deserialize(deserializer)?;

        let frame_id = FrameId::default();
        let mut state = TextureState::new(frame_id);

        if let Some(encoded) = helper.texture_data {
            // Decode base64 string to Vec<u8>
            use base64::Engine;
            match base64::engine::general_purpose::STANDARD.decode(&encoded) {
                Ok(decoded) => {
                    state.texture_data.set(frame_id, decoded);
                }
                Err(_) => {
                    // Invalid base64, leave as default
                }
            }
        }
        if let Some(val) = helper.width {
            state.width.set(frame_id, val);
        }
        if let Some(val) = helper.height {
            state.height.set(frame_id, val);
        }
        if let Some(val) = helper.format {
            state.format.set(frame_id, val);
        }

        Ok(state)
    }
}
