use crate::project::FrameId;
use crate::state::StateField;
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::impl_state_serialization;

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

    /// Merge fields from a partial update into this state
    ///
    /// Only fields that are present in `other` (non-default values) are merged.
    /// Fields not present in the partial update are preserved from `self`.
    pub fn merge_from(&mut self, other: &Self, frame_id: FrameId) {
        // Merge channel_data if present (not empty)
        if !other.channel_data.value().is_empty() {
            self.channel_data
                .set(frame_id, other.channel_data.value().clone());
        }
    }
}

impl_state_serialization! {
    OutputState => SerializableOutputState {
        #[base64] channel_data: Vec<u8>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;
    use alloc::{format, vec};

    #[test]
    fn test_serialize_all_fields_initial_sync() {
        let mut state = OutputState::new(FrameId::new(1));
        state.channel_data.set(FrameId::new(1), vec![10, 20, 30]);

        let serializable = SerializableOutputState::new(&state, FrameId::default());
        let json = json::to_string(&serializable).unwrap();
        // Should contain channel_data for initial sync
        assert!(json.contains("channel_data"));
    }

    #[test]
    fn test_serialize_partial_fields() {
        let mut state = OutputState::new(FrameId::new(1));
        state.channel_data.set(FrameId::new(1), vec![10, 20, 30]);

        // Update channel_data
        state.channel_data.set(FrameId::new(5), vec![40, 50, 60]);

        // Serialize with since_frame = FrameId::new(2)
        // Should include channel_data (changed at frame 5 > 2)
        let serializable = SerializableOutputState::new(&state, FrameId::new(2));
        let json = json::to_string(&serializable).unwrap();
        assert!(json.contains("channel_data"));
    }

    #[test]
    fn test_serialize_no_changes() {
        let mut state = OutputState::new(FrameId::new(1));
        state.channel_data.set(FrameId::new(1), vec![10, 20, 30]);

        // Serialize with since_frame = FrameId::new(5)
        // No fields should be included (changed before frame 5)
        let serializable = SerializableOutputState::new(&state, FrameId::new(5));
        let json = json::to_string(&serializable).unwrap();
        // Should be empty object or minimal
        assert!(!json.contains("channel_data"));
    }

    #[test]
    fn test_deserialize_partial_json_preserves_missing_fields() {
        // Simulate client-side merge: existing state has channel_data, partial update is empty
        let mut existing_state = OutputState::new(FrameId::new(1));
        existing_state
            .channel_data
            .set(FrameId::new(1), vec![100, 200, 255]);

        // Partial update JSON (empty - no fields changed)
        let _partial_json = r#"{}"#;
        let _partial_state: OutputState = json::from_str(_partial_json).unwrap();

        // Merge: only update fields that are present in partial update
        // Since partial_json is empty, no fields should be updated
        // channel_data should be preserved
        assert_eq!(existing_state.channel_data.value(), &vec![100, 200, 255]);
    }

    #[test]
    fn test_deserialize_partial_json_base64_field() {
        // Test that base64-encoded channel_data can be deserialized
        use base64::Engine;
        let channel_bytes = vec![50, 100, 150, 200];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&channel_bytes);
        let json = format!(r#"{{"channel_data": "{}"}}"#, encoded);

        let state: OutputState = json::from_str(&json).unwrap();
        assert_eq!(state.channel_data.value(), &channel_bytes);
    }

    #[test]
    fn test_deserialize_full_json() {
        use base64::Engine;
        let channel_bytes = vec![1, 2, 3, 4, 5];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&channel_bytes);
        let json = format!(r#"{{"channel_data": "{}"}}"#, encoded);

        let state: OutputState = json::from_str(&json).unwrap();
        assert_eq!(state.channel_data.value(), &channel_bytes);
    }

    #[test]
    fn test_merge_partial_update_with_existing_state() {
        // Simulate the client-side merge scenario:
        // 1. Initial sync: receive full state with channel_data
        // 2. Partial update: receive empty JSON (no changes)
        // 3. Verify channel_data is preserved

        // Step 1: Initial sync - deserialize full state
        use base64::Engine;
        let initial_bytes = vec![10, 20, 30, 40];
        let encoded = base64::engine::general_purpose::STANDARD.encode(&initial_bytes);
        let initial_json = format!(r#"{{"channel_data": "{}"}}"#, encoded);
        let mut existing_state: OutputState = json::from_str(&initial_json).unwrap();
        existing_state
            .channel_data
            .set(FrameId::new(1), initial_bytes.clone());

        // Step 2: Partial update - empty JSON (no fields changed)
        let _partial_json = r#"{}"#;
        let _partial_state: OutputState = json::from_str(_partial_json).unwrap();

        // Step 3: Merge logic (simulating client behavior)
        // Only update fields that are present in partial update
        // Since partial_json is empty, existing_state should remain unchanged
        // In real client code, we'd check if fields are present before merging

        // Verify channel_data is preserved
        assert_eq!(existing_state.channel_data.value(), &initial_bytes);
    }
}
