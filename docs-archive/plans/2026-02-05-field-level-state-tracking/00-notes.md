# Field-Level State Tracking Plan

## Scope of Work

Currently, when any field in a node's state changes, the entire `NodeState` is sent to clients. This is inefficient because:

- Some fields change frequently (e.g., `FixtureState::lamp_colors` updates every frame)
- Other fields change rarely (e.g., `FixtureState::mapping_cells` only changes when config/texture changes)
- Sending the entire state every frame wastes bandwidth, especially for large fields like `mapping_cells` or `texture_data`

We need to implement field-level change tracking so that:
- Each field in a state struct tracks which frame it was last changed on
- Only fields that changed since `since_frame` are serialized and sent
- The solution is type-safe for adding new fields
- Clients can merge partial updates with their existing state

## Current State of the Codebase

### State Structure

Node states are defined as structs in `lp-model/src/nodes/*/state.rs`:
- `FixtureState` has: `lamp_colors`, `mapping_cells`, `texture_handle`, `output_handle`
- `TextureState` has: `texture_data`, `width`, `height`, `format`
- `OutputState` has: `channel_data`
- `ShaderState` has: `errors` (and potentially other fields)

### Current Sync Flow

1. `ProjectRuntime::get_changes()` checks if `entry.state_ver > since_frame`
2. If true, the entire `NodeState` is extracted and added to `node_details`
3. `NodeState` is serialized as part of `SerializableNodeDetail`
4. Client receives full state and replaces its cached state

### State Extraction

- States are extracted from node runtimes in `ProjectRuntime::get_changes()` (lines 920-1057)
- For fixtures: `FixtureRuntime::get_lamp_colors()`, `get_mapping()`, etc. are called
- States are converted to `NodeState` enum variants
- Entire state structs are cloned and serialized

### Frame Tracking

- `FrameId` is a wrapper around `i64` that increments each frame
- Each node entry has `state_ver: FrameId` tracking when state last changed
- Frame tracking is at the node level, not field level

## Questions That Need to Be Answered

### Question 1: Wrapper Type Design

**Context:**
We need a wrapper type that tracks which frame a value was changed on. Options:

**Option A: Generic wrapper struct**
```rust
pub struct StateField<T> {
    value: T,
    changed_frame: FrameId,
}
```

**Option B: Separate tracking map**
```rust
// State struct stays the same, but we track changes separately
struct FieldChangeTracker {
    lamp_colors_frame: FrameId,
    mapping_cells_frame: FrameId,
    // ...
}
```

**Option C: Custom serialization with skip logic**
- Keep state structs as-is
- Implement custom `Serialize` that skips fields based on frame tracking
- Track frame changes in a separate structure

**Suggested approach:** Option A (generic wrapper) provides type safety and makes it clear which fields are tracked. However, it requires changing all state structs. Option C might be cleaner for serialization but loses type safety.

**Question:** Which approach should we use? Should we use a generic wrapper, or keep state structs unchanged and use custom serialization?

**Answer:** Option A - Generic wrapper struct. We'll use `StateField<T>` to wrap values and track their change frames. This provides type safety and makes tracking explicit.

### Question 2: State Struct Modification Strategy

**Context:**
If we use wrapper types, we need to decide how to modify state structs:

**Option A: Replace fields with wrappers**
```rust
pub struct FixtureState {
    pub lamp_colors: StateField<Vec<u8>>,
    pub mapping_cells: StateField<Vec<MappingCell>>,
    // ...
}
```

**Option B: Create new "tracked" state structs**
```rust
pub struct FixtureState { /* current */ }
pub struct TrackedFixtureState {
    pub lamp_colors: TrackedField<Vec<u8>>,
    // ...
}
```

**Option C: Use a trait/type alias**
- Keep state structs for internal use
- Create serializable "delta" structs for sync

**Question:** Should we modify existing state structs, create parallel tracked versions, or use a different approach?

**Answer:** Option A - Modify existing state structs. We'll replace field types with `StateField<T>` directly in the existing structs. This keeps a single source of truth and makes tracking explicit. The breaking change is acceptable since state structs are primarily used for serialization.

### Question 3: Frame Update Mechanism

**Context:**
We need to update the `changed_frame` for a field when it changes. This could happen:
- When runtime updates the value (e.g., `FixtureRuntime` updates `lamp_colors`)
- When state is extracted from runtime
- Both (runtime tracks, extraction uses)

**Question:** Where should frame tracking be updated? Should runtimes track field-level changes, or should we detect changes during state extraction?

**Answer:** Runtime directly stores state. The runtime will have `state: FixtureState` as a field, where `FixtureState` fields are `StateField<T>`. When updating values, call `self.state.lamp_colors.set(ctx.frame_id, new_value)` or `self.state.lamp_colors.mark_updated(ctx.frame_id)`. This makes state the single source of truth and frame tracking happens naturally at the point of change.

### Question 4: Partial State Serialization

**Context:**
We need to serialize only changed fields. Options:

**Option A: Custom `Serialize` implementation**
- Implement `Serialize` for tracked state structs
- Skip fields where `changed_frame <= since_frame`

**Option B: Build delta structs**
- Create a `FixtureStateDelta` with `Option<T>` fields
- Only include `Some(value)` for changed fields
- Serialize delta instead of full state

**Option C: Use serde's `skip_serializing_if`**
- Use a function that checks frame ID
- Requires passing `since_frame` context somehow

**Question:** How should we serialize only changed fields? Custom serializer, delta structs, or another approach?

**Answer:** Custom `Serialize` implementation. We'll implement `Serialize` for state structs that checks `changed_frame` against `since_frame` (passed via serialization context) and skips fields that haven't changed. This maintains type safety and avoids duplicate structs.

### Question 5: Client-Side Merging

**Context:**
Clients receive partial state updates and need to merge them with existing state.

**Question:** Should clients:
- Replace entire state (current behavior, but with partial updates)?
- Merge field-by-field (only update fields present in delta)?
- How do we handle fields that weren't sent (assume unchanged vs. need to track what was sent)?

**Answer:** Replace fields that are present. With custom serialization, we serialize only changed fields. On deserialization, serde will only update fields present in JSON. Missing fields remain unchanged, so clients merge automatically.

**Important consideration:** For initial sync (when there's no previous state), we must send all fields since `StateField<T>` values aren't optional. For incremental updates, we only send changed fields. The serialization logic needs to handle both cases - check if this is initial sync (e.g., `since_frame == FrameId::default()`) and send all fields in that case.

**Testing note:** We should write tests with example versions of this before the real implementation to ensure serialization/deserialization works correctly for both initial sync and incremental updates.

### Question 6: Backward Compatibility

**Context:**
We need to ensure existing clients can still work, or we need a migration strategy.

**Question:** Should we:
- Make this opt-in (feature flag, version negotiation)?
- Support both full and partial state in the protocol?
- Break compatibility and require client updates?

**Answer:** Break compatibility - we're still in active development, so we don't need to maintain backward compatibility. We can require client updates to handle the new partial state format.

### Question 7: Type Safety for New Fields

**Context:**
The user wants type safety when adding new fields - the compiler should catch if a new field isn't tracked.

**Question:** How do we ensure type safety? Should we:
- Use a macro to generate tracking code?
- Use a trait that must be implemented?
- Rely on the wrapper type approach (if field is `TrackedField<T>`, it's automatically tracked)?

**Answer:** With the custom serializer approach, we don't need additional type safety mechanisms. The custom serializer will handle all `StateField<T>` fields, and if we forget to handle a field in the serializer implementation, it will be a compile error. The wrapper type approach (`StateField<T>`) is sufficient.

### Question 8: Which Node Types to Track

**Context:**
Not all node types may need field-level tracking. For example:
- `FixtureState`: `lamp_colors` changes every frame, `mapping_cells` rarely
- `TextureState`: `texture_data` might change frequently, `width`/`height` rarely
- `OutputState`: `channel_data` changes every frame
- `ShaderState`: `errors` changes when compilation happens

**Question:** Should we implement field-level tracking for all node types, or start with specific ones (e.g., just `FixtureState`)?

**Answer:** Implement for all node types. Field-level tracking should be the standard way state data is handled. All state structs (`FixtureState`, `TextureState`, `OutputState`, `ShaderState`) should use `TrackedField<T>` for their fields. This provides consistent behavior across all node types.
