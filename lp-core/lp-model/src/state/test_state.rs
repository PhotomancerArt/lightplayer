use crate::project::FrameId;
use crate::state::StateField;
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

/// Test state struct for validating StateField serialization
#[derive(Debug, Clone, PartialEq)]
pub struct TestState {
    pub field1: StateField<String>,
    pub field2: StateField<u32>,
    pub field3: StateField<Vec<u8>>,
}

impl TestState {
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            field1: StateField::new(frame_id, String::from("default")),
            field2: StateField::new(frame_id, 0),
            field3: StateField::new(frame_id, Vec::new()),
        }
    }
}

/// Wrapper for serializing TestState with a since_frame context
pub struct SerializableTestState<'a> {
    state: &'a TestState,
    since_frame: FrameId,
}

impl<'a> SerializableTestState<'a> {
    pub fn new(state: &'a TestState, since_frame: FrameId) -> Self {
        Self { state, since_frame }
    }
}

impl<'a> Serialize for SerializableTestState<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let is_initial_sync = self.since_frame == FrameId::default();
        let mut state = serializer.serialize_struct("TestState", 3)?;

        if is_initial_sync || self.state.field1.changed_frame() > self.since_frame {
            state.serialize_field("field1", self.state.field1.value())?;
        }
        if is_initial_sync || self.state.field2.changed_frame() > self.since_frame {
            state.serialize_field("field2", self.state.field2.value())?;
        }
        if is_initial_sync || self.state.field3.changed_frame() > self.since_frame {
            state.serialize_field("field3", self.state.field3.value())?;
        }

        state.end()
    }
}

impl<'de> Deserialize<'de> for TestState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Use a helper struct with Option fields for deserialization
        #[derive(Deserialize)]
        struct TestStateHelper {
            field1: Option<String>,
            field2: Option<u32>,
            field3: Option<Vec<u8>>,
        }

        let helper = TestStateHelper::deserialize(deserializer)?;

        // Create default state, then merge in provided fields
        // For real implementation, we'd need to know the current frame_id
        // For now, use default - this will be updated when merged with existing state
        let frame_id = FrameId::default();

        let mut state = TestState::new(frame_id);

        if let Some(val) = helper.field1 {
            state.field1.set(frame_id, val);
        }
        if let Some(val) = helper.field2 {
            state.field2.set(frame_id, val);
        }
        if let Some(val) = helper.field3 {
            state.field3.set(frame_id, val);
        }

        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_serialize_all_fields_initial_sync() {
        let state = TestState::new(FrameId::new(1));
        let serializable = SerializableTestState::new(&state, FrameId::default());
        let json = serde_json::to_string(&serializable).unwrap();
        // Should contain all fields for initial sync
        assert!(json.contains("field1"));
        assert!(json.contains("field2"));
        assert!(json.contains("field3"));
    }

    #[test]
    fn test_serialize_partial_fields() {
        let mut state = TestState::new(FrameId::new(1));
        state.field1.set(FrameId::new(5), String::from("updated"));
        // field2 and field3 unchanged (frame 1)

        // Serialize with since_frame = FrameId::new(2)
        // Should only include field1 (changed at frame 5 > 2)
        let serializable = SerializableTestState::new(&state, FrameId::new(2));
        let json = serde_json::to_string(&serializable).unwrap();
        assert!(json.contains("field1"));
        assert!(json.contains("updated"));
        // field2 and field3 should not be present
        assert!(!json.contains("field2"));
        assert!(!json.contains("field3"));
    }

    #[test]
    fn test_serialize_no_changes() {
        let mut state = TestState::new(FrameId::new(1));
        state.field1.set(FrameId::new(2), String::from("updated"));
        // All fields changed at frame 1 or 2

        // Serialize with since_frame = FrameId::new(5)
        // No fields should be included (all changed before frame 5)
        let serializable = SerializableTestState::new(&state, FrameId::new(5));
        let json = serde_json::to_string(&serializable).unwrap();
        // Should be empty object or minimal
        assert!(!json.contains("field1"));
        assert!(!json.contains("field2"));
        assert!(!json.contains("field3"));
    }

    #[test]
    fn test_deserialize_partial_json() {
        let json = r#"{"field1": "test"}"#;
        let state: TestState = serde_json::from_str(json).unwrap();
        assert_eq!(state.field1.value(), "test");
        // field2 and field3 should have default values
        assert_eq!(state.field2.value(), &0);
        assert_eq!(state.field3.value(), &Vec::<u8>::new());
    }

    #[test]
    fn test_deserialize_full_json() {
        let json = r#"{"field1": "test", "field2": 42, "field3": [1, 2, 3]}"#;
        let state: TestState = serde_json::from_str(json).unwrap();
        assert_eq!(state.field1.value(), "test");
        assert_eq!(state.field2.value(), &42);
        assert_eq!(state.field3.value(), &vec![1, 2, 3]);
    }

    #[test]
    fn test_deserialize_empty_json() {
        let json = r#"{}"#;
        let state: TestState = serde_json::from_str(json).unwrap();
        // All fields should have default values
        assert_eq!(state.field1.value(), "default");
        assert_eq!(state.field2.value(), &0);
        assert_eq!(state.field3.value(), &Vec::<u8>::new());
    }
}
