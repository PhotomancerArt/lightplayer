use lpc_model::Versioned;
use lpc_model::project::FrameId;
use lpc_source::legacy::nodes::texture::TextureFormat;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::legacy::compatibility::LegacyCompatBytesField;

/// Texture node state - runtime values
#[derive(Debug, Clone, PartialEq)]
pub struct TextureState {
    /// Texture pixel data (inline base64) or render-product ref; format follows [`Self::format`].
    pub texture_data: LegacyCompatBytesField,
    /// Texture width in pixels
    pub width: Versioned<u32>,
    /// Texture height in pixels
    pub height: Versioned<u32>,
    /// Texture format
    pub format: Versioned<TextureFormat>,
}

impl TextureState {
    /// Create a new TextureState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            texture_data: LegacyCompatBytesField::new(frame_id),
            width: Versioned::new(frame_id, 0),
            height: Versioned::new(frame_id, 0),
            format: Versioned::new(frame_id, TextureFormat::Rgba16),
        }
    }

    /// Merge fields from a partial update into this state
    ///
    /// Only fields that are present in `other` (non-default values) are merged.
    /// Fields not present in the partial update are preserved from `self`.
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        self.texture_data.merge_from(&other.texture_data, frame_id);
        if *other.width.value() != 0 {
            self.width.set(frame_id, *other.width.value());
        }
        if *other.height.value() != 0 {
            self.height.set(frame_id, *other.height.value());
        }
        const DEFAULT_FORMAT: TextureFormat = TextureFormat::Rgba16;
        if other.format.value() != &DEFAULT_FORMAT || other.format.value() == self.format.value() {
            self.format.set(frame_id, *other.format.value());
        }
    }
}

pub struct SerializableTextureState<'a> {
    state: &'a TextureState,
    since_frame: FrameId,
}

impl<'a> SerializableTextureState<'a> {
    pub fn new(state: &'a TextureState, since_frame: FrameId) -> Self {
        Self { state, since_frame }
    }
}

impl Serialize for SerializableTextureState<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let is_initial = self.since_frame == FrameId::default();
        let include_tex = is_initial || self.state.texture_data.changed_frame() > self.since_frame;
        let include_w = is_initial || self.state.width.changed_frame() > self.since_frame;
        let include_h = is_initial || self.state.height.changed_frame() > self.since_frame;
        let include_f = is_initial || self.state.format.changed_frame() > self.since_frame;

        let count = usize::from(include_tex)
            + usize::from(include_w)
            + usize::from(include_h)
            + usize::from(include_f);
        let mut st = serializer.serialize_struct("TextureState", count)?;
        if include_tex {
            st.serialize_field("texture_data", &self.state.texture_data)?;
        }
        if include_w {
            st.serialize_field("width", self.state.width.value())?;
        }
        if include_h {
            st.serialize_field("height", self.state.height.value())?;
        }
        if include_f {
            st.serialize_field("format", self.state.format.value())?;
        }
        st.end()
    }
}

impl Serialize for TextureState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut st = serializer.serialize_struct("TextureState", 4)?;
        st.serialize_field("texture_data", &self.texture_data)?;
        st.serialize_field("width", self.width.value())?;
        st.serialize_field("height", self.height.value())?;
        st.serialize_field("format", self.format.value())?;
        st.end()
    }
}

#[derive(Deserialize)]
struct TextureStateHelper {
    texture_data: Option<LegacyCompatBytesField>,
    width: Option<u32>,
    height: Option<u32>,
    format: Option<TextureFormat>,
}

impl<'de> Deserialize<'de> for TextureState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = TextureStateHelper::deserialize(deserializer)?;
        let frame_id = FrameId::default();
        let mut state = TextureState::new(frame_id);
        if let Some(v) = helper.texture_data {
            state.texture_data = v;
        }
        if let Some(v) = helper.width {
            state.width.set(frame_id, v);
        }
        if let Some(v) = helper.height {
            state.height.set(frame_id, v);
        }
        if let Some(v) = helper.format {
            state.format.set(frame_id, v);
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;
    use alloc::{format, vec};
    use lpc_model::{RenderProductId, ResourceRef};

    #[test]
    fn test_serialize_all_fields_initial_sync() {
        let mut state = TextureState::new(FrameId::new(1));
        state
            .texture_data
            .set_inline(FrameId::new(1), vec![1, 2, 3, 4]);
        state.width.set(FrameId::new(1), 100);
        state.height.set(FrameId::new(1), 200);
        state.format.set(FrameId::new(1), TextureFormat::Rgb8);

        let serializable = SerializableTextureState::new(&state, FrameId::default());
        let json = json::to_string(&serializable).unwrap();
        assert!(json.contains("texture_data"));
        assert!(json.contains("width"));
        assert!(json.contains("height"));
        assert!(json.contains("format"));
    }

    #[test]
    fn test_serialize_partial_fields() {
        let mut state = TextureState::new(FrameId::new(1));
        state
            .texture_data
            .set_inline(FrameId::new(1), vec![1, 2, 3, 4]);
        state.width.set(FrameId::new(1), 100);
        state.height.set(FrameId::new(1), 200);
        state.format.set(FrameId::new(1), TextureFormat::Rgb8);

        state.width.set(FrameId::new(5), 150);
        state.height.set(FrameId::new(5), 250);

        let serializable = SerializableTextureState::new(&state, FrameId::new(2));
        let json = json::to_string(&serializable).unwrap();
        assert!(json.contains("width"));
        assert!(json.contains("height"));
        assert!(!json.contains("texture_data"));
        assert!(!json.contains("format"));
    }

    #[test]
    fn test_deserialize_partial_json_preserves_missing_fields() {
        let mut existing_state = TextureState::new(FrameId::new(1));
        existing_state
            .texture_data
            .set_inline(FrameId::new(1), vec![10, 20, 30, 40]);
        existing_state.width.set(FrameId::new(1), 100);
        existing_state.height.set(FrameId::new(1), 200);
        existing_state
            .format
            .set(FrameId::new(1), TextureFormat::Rgb8);

        let partial_json = r#"{"width": 150, "height": 250}"#;
        let partial_state: TextureState = json::from_str(partial_json).unwrap();

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

        assert_eq!(existing_state.width.value(), &150);
        assert_eq!(existing_state.height.value(), &250);
        assert_eq!(
            existing_state.texture_data.inline_bytes(),
            &[10, 20, 30, 40]
        );
        assert_eq!(existing_state.format.value(), &TextureFormat::Rgb8);
    }

    #[test]
    fn test_deserialize_partial_json_base64_field() {
        use base64::Engine;
        let texture_bytes = vec![100, 200, 255, 128];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&texture_bytes);
        let json = format!(r#"{{"texture_data": "{}"}}"#, encoded);

        let state: TextureState = json::from_str(&json).unwrap();
        assert_eq!(state.texture_data.inline_bytes(), texture_bytes.as_slice());
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
        assert_eq!(state.texture_data.inline_bytes(), texture_bytes.as_slice());
        assert_eq!(state.width.value(), &100);
        assert_eq!(state.height.value(), &200);
        assert_eq!(state.format.value(), &TextureFormat::Rgb8);
    }

    #[test]
    fn texture_data_render_product_ref_roundtrip() {
        let mut state = TextureState::new(FrameId::new(1));
        let rid = ResourceRef::render_product(RenderProductId::new(3));
        state.texture_data.set_resource(FrameId::new(4), rid);
        let j = json::to_string(&state).unwrap();
        assert!(j.contains("texture_data"));
        assert!(j.contains("$lp:res/render_product/3"));
        let back: TextureState = json::from_str(&j).unwrap();
        assert_eq!(back.texture_data.resource_ref(), Some(rid));
    }
}
