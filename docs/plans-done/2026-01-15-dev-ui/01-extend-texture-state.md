# Phase 1: Extend TextureState with width, height, format

## Goal

Add width, height, and format fields to `TextureState` to enable proper texture display in the debug UI.

## Tasks

1. Update `lp-model/src/nodes/texture/state.rs`:
   - Add `width: u32` field
   - Add `height: u32` field
   - Add `format: String` field
   - Update struct documentation

2. Ensure serialization/deserialization works:
   - Verify `#[derive(Serialize, Deserialize)]` handles new fields
   - Test that existing code still compiles

## Success Criteria

- `TextureState` includes width, height, and format fields
- Code compiles without warnings
- Serialization/deserialization works correctly
- All existing tests pass

## Implementation Notes

- Format should be a string like "RGB8", "RGBA8", "R8" to match existing texture format handling
- These fields will be populated by the engine runtime in a later phase
