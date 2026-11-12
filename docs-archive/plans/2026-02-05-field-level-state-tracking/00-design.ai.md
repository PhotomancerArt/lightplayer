# Design: Field-Level State Tracking

## Scope of Work

Implement field-level change tracking for node state so that only changed fields are serialized and sent to clients. This reduces bandwidth usage, especially for states with large fields that change infrequently (e.g., `mapping_cells` in `FixtureState`).

## Architecture Overview

### Core Concept

1. **StateField<T>**: A wrapper type that stores a value and the frame ID when it was last changed
2. **State structs**: Modified to use `StateField<T>` for all fields
3. **Runtime storage**: Runtimes store state directly (e.g., `FixtureRuntime` has `state: FixtureState`)
4. **Frame tracking**: When updating values, call `self.state.field.set(ctx.frame_id, value)` or `self.state.field.mark_updated(ctx.frame_id)`
5. **Custom serialization**: Implement `Serialize` that skips fields where `changed_frame <= since_frame`
6. **Initial sync**: When `since_frame == FrameId::default()`, send all fields

### Key Design Decisions

- **Runtime owns state**: State is stored directly in runtime, not extracted on-demand
- **Type-safe tracking**: All state fields use `StateField<T>`, ensuring they're tracked
- **Custom serialization**: Custom `Serialize` implementation handles partial serialization
- **No backward compatibility**: We're in active development, so we can break compatibility

## File Structure

```
lp-model/src/
├── nodes/
│   ├── fixture/
│   │   └── state.rs                    # MODIFY: Use StateField<T> for all fields
│   ├── texture/
│   │   └── state.rs                    # MODIFY: Use StateField<T> for all fields
│   ├── output/
│   │   └── state.rs                    # MODIFY: Use StateField<T> for all fields
│   └── shader/
│       └── state.rs                    # MODIFY: Use StateField<T> for all fields
└── state/
    ├── mod.rs                          # NEW: StateField<T> definition
    └── state_field.rs                  # NEW: StateField implementation

lp-engine/src/
├── nodes/
│   ├── fixture/
│   │   └── runtime.rs                  # MODIFY: Store state directly, update via StateField methods
│   ├── texture/
│   │   └── runtime.rs                  # MODIFY: Store state directly, update via StateField methods
│   ├── output/
│   │   └── runtime.rs                  # MODIFY: Store state directly, update via StateField methods
│   └── shader/
│       └── runtime.rs                 # MODIFY: Store state directly, update via StateField methods
├── project/
│   └── runtime.rs                      # MODIFY: State extraction now just clones state
└── runtime/
    └── contexts.rs                     # MODIFY: Add frame_id to RenderContext

lp-model/src/state/
└── serialization.rs                    # NEW: Custom Serialize implementation for tracked states
```

## Conceptual Architecture

### StateField<T>

```rust
pub struct StateField<T> {
    value: T,
    changed_frame: FrameId,
}

impl<T> StateField<T> {
    pub fn new(frame_id: FrameId, value: T) -> Self;
    pub fn get(&self) -> &T;
    pub fn get_mut(&mut self) -> &mut T;
    pub fn set(&mut self, frame_id: FrameId, value: T);
    pub fn mark_updated(&mut self, frame_id: FrameId);
    pub fn changed_frame(&self) -> FrameId;
    pub fn value(&self) -> &T;
    pub fn into_value(self) -> T;
}
```

### State Struct Example (FixtureState)

```rust
pub struct FixtureState {
    pub lamp_colors: StateField<Vec<u8>>,
    pub mapping_cells: StateField<Vec<MappingCell>>,
    pub texture_handle: StateField<Option<NodeHandle>>,
    pub output_handle: StateField<Option<NodeHandle>>,
}
```

### Runtime Storage Example (FixtureRuntime)

```rust
pub struct FixtureRuntime {
    config: Option<FixtureConfig>,
    state: FixtureState,  // State stored directly
    // Implementation details (not in state):
    color_order: ColorOrder,
    mapping: Vec<MappingPoint>,
    transform: [[f32; 4]; 4],
    texture_width: Option<u32>,
    texture_height: Option<u32>,
    precomputed_mapping: Option<PrecomputedMapping>,
    brightness: u8,
    gamma_correction: bool,
}
```

### Update Pattern

```rust
// When updating lamp_colors:
self.state.lamp_colors.set(ctx.frame_id, new_colors);

// When updating mapping_cells (derived from internal mapping):
let mapping_cells = convert_mapping_to_cells(&self.mapping);
self.state.mapping_cells.set(ctx.frame_id, mapping_cells);

// When just marking as updated (value changed externally):
self.state.texture_handle.mark_updated(ctx.frame_id);
```

### Custom Serialization

```rust
impl Serialize for FixtureState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Get since_frame from serialization context
        let since_frame = /* get from context */;
        let is_initial_sync = since_frame == FrameId::default();
        
        let mut state = serializer.serialize_struct("FixtureState", /* count */)?;
        
        // Only serialize fields that changed (or all fields for initial sync)
        if is_initial_sync || self.lamp_colors.changed_frame() > since_frame {
            state.serialize_field("lamp_colors", &self.lamp_colors.value())?;
        }
        // ... repeat for other fields
        
        state.end()
    }
}
```

## Main Components and Interactions

### 1. StateField<T>
- Wraps a value and tracks when it was last changed
- Provides methods to get/set values and update frame tracking
- Used by all state struct fields

### 2. State Structs (FixtureState, TextureState, etc.)
- All fields are `StateField<T>`
- Implement custom `Serialize` that skips unchanged fields
- Implement `Deserialize` normally (all fields may be present or missing)

### 3. Node Runtimes
- Store state directly as a field (e.g., `state: FixtureState`)
- Update state via `StateField` methods when values change
- State extraction is now just cloning the state field

### 4. RenderContext
- Extended to include `frame_id: FrameId`
- Passed to runtime update methods
- Used when updating tracked fields

### 5. Serialization Context
- Custom serialization needs `since_frame` to determine which fields to skip
- Use serde's serialization context or a thread-local to pass `since_frame`
- For initial sync (`since_frame == FrameId::default()`), serialize all fields

### 6. Client Deserialization
- Deserialize partial JSON into full state struct
- Missing fields in JSON are left unchanged (serde default behavior)
- Clients merge partial updates automatically

## State vs Implementation Details

**State (synced to clients):**
- `FixtureState`: `lamp_colors`, `mapping_cells`, `texture_handle`, `output_handle`
- `TextureState`: `texture_data`, `width`, `height`, `format`
- `OutputState`: `channel_data`
- `ShaderState`: `errors`, etc.

**Implementation Details (not synced):**
- `FixtureRuntime`: `precomputed_mapping`, `transform`, `texture_width`, `texture_height`, `color_order`, `brightness`, `gamma_correction`, `mapping` (internal representation)
- These remain as regular fields in runtime, not in state structs

## Initial Sync Handling

When `since_frame == FrameId::default()` (initial sync):
- Serialize all fields regardless of `changed_frame`
- Ensures client has complete state on first sync
- Subsequent syncs only send changed fields

## Testing Strategy

Before full implementation, create example/test versions to validate:
1. `StateField<T>` behavior (get, set, mark_updated)
2. Custom serialization (skips unchanged fields, includes all for initial sync)
3. Deserialization (handles partial JSON, merges with existing state)
4. Frame tracking (correctly tracks when fields change)
