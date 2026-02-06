# Phase 3: Create Test/Example State Struct with StateField to Validate Serialization

## Scope of Phase

Before refactoring real state structs, create a simple test state struct with `StateField<T>` fields and implement custom serialization to validate the approach works correctly. This will test:
- Serialization that skips unchanged fields
- Serialization that includes all fields for initial sync
- Deserialization that handles partial JSON
- Frame tracking integration

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Create test state struct

Create `lp-model/src/state/test_state.rs`:

```rust
use crate::project::FrameId;
use crate::state::StateField;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
```

### 2. Implement custom Serialize

Implement `Serialize` that skips unchanged fields:

```rust
impl Serialize for TestState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // TODO: Get since_frame from serialization context
        // For now, use a thread-local or parameter
        // This is a placeholder - we'll need to figure out how to pass since_frame
        let since_frame = FrameId::default(); // Placeholder
        
        let is_initial_sync = since_frame == FrameId::default();
        let mut state = serializer.serialize_struct("TestState", 3)?;
        
        if is_initial_sync || self.field1.changed_frame() > since_frame {
            state.serialize_field("field1", self.field1.value())?;
        }
        if is_initial_sync || self.field2.changed_frame() > since_frame {
            state.serialize_field("field2", self.field2.value())?;
        }
        if is_initial_sync || self.field3.changed_frame() > since_frame {
            state.serialize_field("field3", self.field3.value())?;
        }
        
        state.end()
    }
}
```

**Note:** We need to figure out how to pass `since_frame` to the serializer. Options:
- Use serde's `SerializeSeed` with a context
- Use a thread-local (less ideal)
- Pass via a wrapper type

For this test phase, we can use a simple approach and refine it later.

### 3. Implement Deserialize

Implement `Deserialize` that handles partial JSON:

```rust
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
        let frame_id = FrameId::default(); // Placeholder
        
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
```

### 4. Add tests

Add comprehensive tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_serialize_all_fields_initial_sync() {
        let state = TestState::new(FrameId::new(1));
        let json = serde_json::to_string(&state).unwrap();
        // Should contain all fields
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
        // Should only include field1
        // TODO: Implement proper serialization context passing
    }

    #[test]
    fn test_deserialize_partial_json() {
        let json = r#"{"field1": "test"}"#;
        let state: TestState = serde_json::from_str(json).unwrap();
        assert_eq!(state.field1.value(), "test");
        // field2 and field3 should have default values
    }

    #[test]
    fn test_deserialize_full_json() {
        let json = r#"{"field1": "test", "field2": 42, "field3": [1, 2, 3]}"#;
        let state: TestState = serde_json::from_str(json).unwrap();
        assert_eq!(state.field1.value(), "test");
        assert_eq!(state.field2.value(), &42);
        assert_eq!(state.field3.value(), &vec![1, 2, 3]);
    }
}
```

### 5. Research serialization context passing

Investigate how to pass `since_frame` to the serializer. Options to explore:
- `SerializeSeed` - serde's mechanism for context-aware serialization
- Wrapper type that includes `since_frame`
- Thread-local (less ideal but simpler)

Document findings and choose an approach.

## Validate

Run the following commands to validate this phase:

```bash
cd lp-core/lp-model
cargo test state::test_state
cargo check
```

Fix any warnings or errors before proceeding. The test state should serialize/deserialize correctly, even if the `since_frame` passing mechanism is still being refined.
