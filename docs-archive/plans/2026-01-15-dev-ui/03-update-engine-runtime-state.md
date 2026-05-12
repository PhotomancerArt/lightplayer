# Phase 3: Update engine runtime state extraction

## Goal

Update the engine runtime to extract width, height, format for `TextureState` and mapping_cells for `FixtureState` when creating node state.

## Tasks

1. Update `lp-engine/src/project/runtime.rs`:
   - In `get_changes()`, when creating `TextureState`:
     - Extract width, height from runtime texture
     - Extract format from runtime texture (default to "RGBA8" if not available)
     - Include in `TextureState` creation
   
   - In `get_changes()`, when creating `FixtureState`:
     - Get mapping points from fixture runtime
     - Apply transform matrix to convert from fixture space to texture space
     - Normalize coordinates to [0, 1] range
     - Create `MappingCell` instances
     - Include in `FixtureState` creation

2. Add helper function for transform application if needed:
   - Convert mapping points from fixture space to texture space
   - Apply 4x4 transform matrix
   - Normalize coordinates

## Success Criteria

- TextureState includes width, height, format from runtime
- FixtureState includes mapping_cells with post-transform coordinates
- Transform application works correctly
- Code compiles without warnings
- All existing tests pass

## Implementation Notes

- For texture format, check if runtime texture has format info, default to "RGBA8"
- For fixture mapping, get mapping from `FixtureRuntime` (stored in `mapping` field)
- Transform matrix is in `FixtureConfig`, apply to mapping center points
- Convert from fixture space (may be [-1,1] or other) to texture space [0,1]
