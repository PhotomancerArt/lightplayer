# Phase 8: Update ProjectRuntime State Extraction and Serialization

## Scope of Phase

Update `ProjectRuntime::get_changes()` to properly pass `since_frame` to state serialization, and ensure the custom serialization works correctly for partial state updates.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Implement serialization context passing

Based on findings from Phase 3, implement a way to pass `since_frame` to state serialization. Options:

**Option A: Use SerializeSeed**
```rust
struct StateSerializerSeed {
    since_frame: FrameId,
}

impl<T: Serialize> SerializeSeed for StateSerializerSeed {
    type Value = T;
    // ...
}
```

**Option B: Wrapper type**
```rust
struct SerializableState<T> {
    state: T,
    since_frame: FrameId,
}

impl<T: Serialize> Serialize for SerializableState<T> {
    // Custom serialization that uses since_frame
}
```

**Option C: Thread-local (less ideal)**
```rust
thread_local! {
    static SINCE_FRAME: RefCell<Option<FrameId>> = RefCell::new(None);
}
```

Choose the approach that works best based on Phase 3 findings.

### 2. Update `ProjectRuntime::get_changes()`

Modify state extraction to use the serialization context:

```rust
pub fn get_changes(
    &self,
    since_frame: FrameId,
    detail_specifier: &ApiNodeSpecifier,
    theoretical_fps: Option<f32>,
) -> Result<ProjectResponse, Error> {
    // ... existing code ...
    
    // When serializing state, pass since_frame context
    for (handle, entry) in &self.nodes {
        if detail_handles.contains(handle) {
            let state = match entry.kind {
                NodeKind::Fixture => {
                    if let Some(runtime) = &entry.runtime {
                        if let Some(fixture_runtime) = runtime.as_any().downcast_ref::<FixtureRuntime>() {
                            // Wrap state with serialization context
                            SerializableState {
                                state: &fixture_runtime.state,
                                since_frame,
                            }
                        } else {
                            // Fallback
                        }
                    } else {
                        // Fallback
                    }
                }
                // ... other node types
            };
            
            // Serialize with context
            let serialized_state = /* serialize with since_frame context */;
            
            // Add to node_details
        }
    }
    
    // ... rest of method ...
}
```

### 3. Update `SerializableNodeDetail`

Ensure `SerializableNodeDetail` can handle partial state serialization. The state field should use the custom serialization that respects `since_frame`.

### 4. Update `to_serializable()` method

In `lp-model/src/project/api.rs`, update `NodeDetail::to_serializable()` to pass `since_frame` when serializing state:

```rust
impl NodeDetail {
    pub fn to_serializable_with_frame(&self, since_frame: FrameId) -> Result<SerializableNodeDetail, String> {
        // Serialize state with since_frame context
        // ...
    }
}
```

### 5. Handle initial sync

Ensure that when `since_frame == FrameId::default()`, all fields are serialized regardless of their `changed_frame`.

### 6. Update client deserialization

Ensure clients can deserialize partial state updates correctly. The `Deserialize` implementations should handle missing fields gracefully.

## Validate

Run the following commands to validate this phase:

```bash
cd lp-core/lp-engine
cargo test project::runtime
cargo check

cd lp-core/lp-model
cargo test project::api
cargo check

cd lp-core/lp-client
cargo test
cargo check
```

Fix any warnings or errors before proceeding. State serialization should now only send changed fields (except for initial sync).
