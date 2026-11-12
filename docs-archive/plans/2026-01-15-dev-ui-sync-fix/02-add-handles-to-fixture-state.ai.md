# Phase 2: Add resolved handles to FixtureState

## Goal

Add `texture_handle` and `output_handle` fields to `FixtureState` and extract them from `FixtureRuntime` when creating state.

## Tasks

1. Update `lp-app/crates/lp-model/src/nodes/fixture/state.rs`:
   - Add `texture_handle: Option<NodeHandle>` field to `FixtureState`
   - Add `output_handle: Option<NodeHandle>` field to `FixtureState`
   - Update serialization/deserialization if needed

2. Update `lp-app/crates/lp-engine/src/nodes/fixture/runtime.rs`:
   - Add `pub fn get_texture_handle(&self) -> Option<TextureHandle>` method
   - Add `pub fn get_output_handle(&self) -> Option<OutputHandle>` method
   - Return handles from internal fields

3. Update `lp-app/crates/lp-engine/src/project/runtime.rs`:
   - In `get_changes()`, when creating `FixtureState`:
     - Extract `texture_handle` from `FixtureRuntime` if available
     - Extract `output_handle` from `FixtureRuntime` if available
     - Convert `TextureHandle`/`OutputHandle` to `NodeHandle` (they wrap `NodeHandle`)
     - Include handles in `FixtureState` creation

4. Update any tests that create `FixtureState`:
   - Add `texture_handle: None` and `output_handle: None` to test fixtures

## Success Criteria

- `FixtureState` includes `texture_handle` and `output_handle` fields
- Handles are extracted from runtime when available
- Handles are `None` when fixture hasn't been initialized
- Code compiles without errors
- Tests pass

## Implementation Notes

- `TextureHandle` and `OutputHandle` wrap `NodeHandle`, so we can extract the inner handle
- Handle case where runtime doesn't exist or handles are `None`
- Handles will be `None` if fixture hasn't been initialized yet
- This is fine - UI should handle `None` gracefully
