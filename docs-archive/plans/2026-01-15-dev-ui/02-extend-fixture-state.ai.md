# Phase 2: Extend FixtureState with MappingCell and mapping_cells

## Goal

Add `MappingCell` struct and `mapping_cells` field to `FixtureState` to enable fixture mapping overlay display in the debug UI.

## Tasks

1. Create `lp-model/src/nodes/fixture/state.rs` (or update if exists):

   - Define `MappingCell` struct:
     ```rust
     pub struct MappingCell {
         pub channel: u32,
         pub center: [f32; 2],  // Post-transform, in texture space [0,1]
         pub radius: f32,
     }
     ```
   - Add `mapping_cells: Vec<MappingCell>` to `FixtureState`
   - Ensure `MappingCell` has `Serialize` and `Deserialize` derives
   - Update struct documentation

2. Export `MappingCell` from `lp-model/src/nodes/fixture/mod.rs` if needed

## Success Criteria

- `MappingCell` struct exists with correct fields
- `FixtureState` includes `mapping_cells` field
- Code compiles without warnings
- Serialization/deserialization works correctly
- All existing tests pass

## Implementation Notes

- Mapping cells represent post-transform sampling regions
- Center coordinates are in normalized texture space [0, 1]
- These will be populated by the engine runtime in the next phase
