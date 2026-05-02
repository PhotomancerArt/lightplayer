use lpc_model::project::FrameId;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::legacy::compatibility::LegacyCompatBytesField;

/// Output node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputState {
    /// Channel data: inline high byte per channel (RGB) or a runtime-buffer [`ResourceRef`](lpc_model::ResourceRef).
    pub channel_data: LegacyCompatBytesField,
}

impl OutputState {
    /// Create a new OutputState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            channel_data: LegacyCompatBytesField::new(frame_id),
        }
    }

    /// Merge fields from a partial update into this state
    ///
    /// Only fields that are present in `other` (non-default values) are merged.
    /// Fields not present in the partial update are preserved from `self`.
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        self.channel_data.merge_from(&other.channel_data, frame_id);
    }
}

/// Wrapper for serializing [`OutputState`] with a since_frame context
pub struct SerializableOutputState<'a> {
    state: &'a OutputState,
    since_frame: FrameId,
}

impl<'a> SerializableOutputState<'a> {
    pub fn new(state: &'a OutputState, since_frame: FrameId) -> Self {
        Self { state, since_frame }
    }
}

impl Serialize for SerializableOutputState<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let is_initial_sync = self.since_frame == FrameId::default();
        let include_channel =
            is_initial_sync || self.state.channel_data.changed_frame() > self.since_frame;
        let count = usize::from(include_channel);
        let mut st = serializer.serialize_struct("OutputState", count)?;
        if include_channel {
            st.serialize_field("channel_data", &self.state.channel_data)?;
        }
        st.end()
    }
}

impl Serialize for OutputState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut st = serializer.serialize_struct("OutputState", 1)?;
        st.serialize_field("channel_data", &self.channel_data)?;
        st.end()
    }
}

#[derive(Deserialize)]
struct OutputStateHelper {
    channel_data: Option<LegacyCompatBytesField>,
}

impl<'de> Deserialize<'de> for OutputState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = OutputStateHelper::deserialize(deserializer)?;
        let frame_id = FrameId::default();
        let mut state = OutputState::new(frame_id);
        if let Some(v) = helper.channel_data {
            state.channel_data = v;
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;
    use alloc::{format, vec};
    use lpc_model::{ResourceDomain, ResourceRef, RuntimeBufferId};

    #[test]
    fn test_serialize_all_fields_initial_sync() {
        let mut state = OutputState::new(FrameId::new(1));
        state
            .channel_data
            .set_inline(FrameId::new(1), vec![10, 20, 30]);

        let serializable = SerializableOutputState::new(&state, FrameId::default());
        let json = json::to_string(&serializable).unwrap();
        assert!(json.contains("channel_data"));
    }

    #[test]
    fn test_serialize_partial_fields() {
        let mut state = OutputState::new(FrameId::new(1));
        state
            .channel_data
            .set_inline(FrameId::new(1), vec![10, 20, 30]);

        state
            .channel_data
            .set_inline(FrameId::new(5), vec![40, 50, 60]);

        let serializable = SerializableOutputState::new(&state, FrameId::new(2));
        let json = json::to_string(&serializable).unwrap();
        assert!(json.contains("channel_data"));
    }

    #[test]
    fn test_serialize_no_changes() {
        let mut state = OutputState::new(FrameId::new(1));
        state
            .channel_data
            .set_inline(FrameId::new(1), vec![10, 20, 30]);

        let serializable = SerializableOutputState::new(&state, FrameId::new(5));
        let json = json::to_string(&serializable).unwrap();
        assert!(!json.contains("channel_data"));
    }

    #[test]
    fn test_deserialize_partial_json_preserves_missing_fields() {
        let mut existing_state = OutputState::new(FrameId::new(1));
        existing_state
            .channel_data
            .set_inline(FrameId::new(1), vec![100, 200, 255]);

        let _partial_json = r#"{}"#;
        let _partial_state: OutputState = json::from_str(_partial_json).unwrap();

        assert_eq!(existing_state.channel_data.inline_bytes(), &[100, 200, 255]);
    }

    #[test]
    fn test_deserialize_partial_json_base64_field() {
        use base64::Engine;
        let channel_bytes = vec![50, 100, 150, 200];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&channel_bytes);
        let json = format!(r#"{{"channel_data": "{}"}}"#, encoded);

        let state: OutputState = json::from_str(&json).unwrap();
        assert_eq!(state.channel_data.inline_bytes(), channel_bytes.as_slice());
    }

    #[test]
    fn test_deserialize_full_json() {
        use base64::Engine;
        let channel_bytes = vec![1, 2, 3, 4, 5];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&channel_bytes);
        let json = format!(r#"{{"channel_data": "{}"}}"#, encoded);

        let state: OutputState = json::from_str(&json).unwrap();
        assert_eq!(state.channel_data.inline_bytes(), channel_bytes.as_slice());
    }

    #[test]
    fn test_merge_partial_update_with_existing_state() {
        use base64::Engine;
        let initial_bytes = vec![10, 20, 30, 40];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&initial_bytes);
        let initial_json = format!(r#"{{"channel_data": "{}"}}"#, encoded);
        let mut existing_state: OutputState = json::from_str(&initial_json).unwrap();
        existing_state
            .channel_data
            .set_inline(FrameId::new(1), initial_bytes.clone());

        let _partial_json = r#"{}"#;
        let _partial_state: OutputState = json::from_str(_partial_json).unwrap();

        assert_eq!(
            existing_state.channel_data.inline_bytes(),
            initial_bytes.as_slice()
        );
    }

    #[test]
    fn test_channel_data_resource_ref_roundtrip() {
        let mut state = OutputState::new(FrameId::new(1));
        let rid = ResourceRef::runtime_buffer(RuntimeBufferId::new(7));
        state.channel_data.set_resource(FrameId::new(3), rid);

        let json = json::to_string(&state).unwrap();
        assert!(json.contains("channel_data"));
        assert!(json.contains("$lp:res/runtime_buffer/7"));

        let back: OutputState = json::from_str(&json).unwrap();
        assert_eq!(back.channel_data.resource_ref(), Some(rid));
        assert!(back.channel_data.inline_bytes().is_empty());
    }

    #[test]
    fn merge_omitted_channel_data_preserves_existing() {
        let f = FrameId::new(10);
        let mut existing = OutputState::new(f);
        existing.channel_data.set_inline(f, vec![1, 2, 3]);
        let partial = OutputState::new(FrameId::default());
        existing.merge_from(&partial, FrameId::new(11));
        assert_eq!(existing.channel_data.inline_bytes(), &[1, 2, 3]);
    }

    #[test]
    fn merge_resource_ref_over_inline() {
        let f = FrameId::new(1);
        let mut existing = OutputState::new(f);
        existing.channel_data.set_inline(f, vec![9, 9, 9]);
        let rid = ResourceRef {
            domain: ResourceDomain::RuntimeBuffer,
            id: 42,
        };
        let mut partial = OutputState::new(f);
        partial.channel_data.set_resource(FrameId::new(2), rid);
        existing.merge_from(&partial, FrameId::new(3));
        assert_eq!(existing.channel_data.resource_ref(), Some(rid));
    }
}
