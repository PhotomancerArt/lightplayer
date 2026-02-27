use crate::project::FrameId;
use crate::state::StateField;
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use super::format::TextureFormat;
use crate::impl_state_serialization;

/// Texture node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureState {
    /// Texture pixel data. Format-agnostic raw bytes; interpretation depends on `format`.
    /// For Rgba16: 8 bytes/pixel (little-endian u16 RGBA). Serialized as base64.
    pub texture_data: StateField<Vec<u8>>,
    /// Texture width in pixels
    pub width: StateField<u32>,
    /// Texture height in pixels
    pub height: StateField<u32>,
    /// Texture format
    pub format: StateField<TextureFormat>,
}

impl TextureState {
    /// Create a new TextureState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            texture_data: StateField::new(frame_id, Vec::new()),
            width: StateField::new(frame_id, 0),
            height: StateField::new(frame_id, 0),
            format: StateField::new(frame_id, TextureFormat::Rgba16),
        }
    }

    /// Merge fields from a partial update into this state
    ///
    /// Only fields that are present in `other` (non-default values) are merged.
    /// Fields not present in the partial update are preserved from `self`.
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        // Merge texture_data if present (not empty)
        if !other.texture_data.value().is_empty() {
            self.texture_data
                .set(frame_id, other.texture_data.value().clone());
        }
        // Merge width if present (not zero)
        if *other.width.value() != 0 {
            self.width.set(frame_id, *other.width.value());
        }
        // Merge height if present (not zero)
        if *other.height.value() != 0 {
            self.height.set(frame_id, *other.height.value());
        }
        // Merge format if present (not the default, or matches current)
        // Default is Rgba16, so if other has default and self has different value, preserve self
        const DEFAULT_FORMAT: TextureFormat = TextureFormat::Rgba16;
        if other.format.value() != &DEFAULT_FORMAT || other.format.value() == self.format.value() {
            // Field was in JSON (non-default) or matches current value, merge it
            self.format.set(frame_id, *other.format.value());
        }
        // Otherwise, format has default value and differs from self, so it wasn't in JSON - preserve self
    }
}

impl_state_serialization! {
    TextureState => SerializableTextureState {
        #[base64] texture_data: Vec<u8>,
        width: u32,
        height: u32,
        format: TextureFormat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;
    use alloc::{format, vec};

    #[test]
    fn test_serialize_all_fields_initial_sync() {
        let mut state = TextureState::new(FrameId::new(1));
        state.texture_data.set(FrameId::new(1), vec![1, 2, 3, 4]);
        state.width.set(FrameId::new(1), 100);
        state.height.set(FrameId::new(1), 200);
        state.format.set(FrameId::new(1), TextureFormat::Rgb8);

        let serializable = SerializableTextureState::new(&state, FrameId::default());
        let json = json::to_string(&serializable).unwrap();
        // Should contain all fields for initial sync
        assert!(json.contains("texture_data"));
        assert!(json.contains("width"));
        assert!(json.contains("height"));
        assert!(json.contains("format"));
    }

    #[test]
    fn test_serialize_partial_fields() {
        let mut state = TextureState::new(FrameId::new(1));
        state.texture_data.set(FrameId::new(1), vec![1, 2, 3, 4]);
        state.width.set(FrameId::new(1), 100);
        state.height.set(FrameId::new(1), 200);
        state.format.set(FrameId::new(1), TextureFormat::Rgb8);

        // Update only width and height
        state.width.set(FrameId::new(5), 150);
        state.height.set(FrameId::new(5), 250);
        // texture_data and format unchanged (frame 1)

        // Serialize with since_frame = FrameId::new(2)
        // Should only include width and height (changed at frame 5 > 2)
        let serializable = SerializableTextureState::new(&state, FrameId::new(2));
        let json = json::to_string(&serializable).unwrap();
        assert!(json.contains("width"));
        assert!(json.contains("height"));
        // texture_data and format should not be present
        assert!(!json.contains("texture_data"));
        assert!(!json.contains("format"));
    }

    #[test]
    fn test_deserialize_partial_json_preserves_missing_fields() {
        // Simulate client-side merge: existing state has texture_data, partial update only has width/height
        let mut existing_state = TextureState::new(FrameId::new(1));
        existing_state
            .texture_data
            .set(FrameId::new(1), vec![10, 20, 30, 40]);
        existing_state.width.set(FrameId::new(1), 100);
        existing_state.height.set(FrameId::new(1), 200);
        existing_state
            .format
            .set(FrameId::new(1), TextureFormat::Rgb8);

        // Partial update JSON (only width and height changed)
        let partial_json = r#"{"width": 150, "height": 250}"#;
        let partial_state: TextureState = json::from_str(partial_json).unwrap();

        // Merge: update fields that are present in partial update
        let current_frame = FrameId::new(5);
        if partial_state.width.value() != existing_state.width.value() {
            existing_state
                .width
                .set(current_frame, *partial_state.width.value());
        }
        if partial_state.height.value() != existing_state.height.value() {
            existing_state
                .height
                .set(current_frame, *partial_state.height.value());
        }

        // Verify merged state: width/height updated, texture_data preserved
        assert_eq!(existing_state.width.value(), &150);
        assert_eq!(existing_state.height.value(), &250);
        assert_eq!(existing_state.texture_data.value(), &vec![10, 20, 30, 40]);
        assert_eq!(existing_state.format.value(), &TextureFormat::Rgb8);
    }

    #[test]
    fn test_deserialize_partial_json_base64_field() {
        // Test that base64-encoded texture_data can be deserialized
        use base64::Engine;
        let texture_bytes = vec![100, 200, 255, 128];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&texture_bytes);
        let json = format!(r#"{{"texture_data": "{}"}}"#, encoded);

        let state: TextureState = json::from_str(&json).unwrap();
        assert_eq!(state.texture_data.value(), &texture_bytes);
        // Other fields should have defaults
        assert_eq!(state.width.value(), &0);
        assert_eq!(state.height.value(), &0);
    }

    #[test]
    fn test_deserialize_full_json() {
        use base64::Engine;
        let texture_bytes = vec![1, 2, 3, 4];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&texture_bytes);
        let json = format!(
            r#"{{"texture_data": "{}", "width": 100, "height": 200, "format": "RGB8"}}"#,
            encoded
        );

        let state: TextureState = json::from_str(&json).unwrap();
        assert_eq!(state.texture_data.value(), &texture_bytes);
        assert_eq!(state.width.value(), &100);
        assert_eq!(state.height.value(), &200);
        assert_eq!(state.format.value(), &TextureFormat::Rgb8);
    }
}
