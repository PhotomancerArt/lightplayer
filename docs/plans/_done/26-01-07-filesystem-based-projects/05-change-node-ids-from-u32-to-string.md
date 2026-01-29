# Phase 5: Change Node IDs from u32 to String

## Goal

Change all node IDs from `u32` to `String` (path-based IDs).

## Tasks

1. Update `lp-core/src/nodes/id.rs`:
   - Change `TextureId`, `OutputId`, `ShaderId`, `FixtureId` from `u32` wrapper to `String` wrapper
   - Update `From<u32>` and `Into<u32>` implementations (remove or update as needed)
   - Keep `Serialize`/`Deserialize` implementations

2. Update all node config types:
   - `ShaderNode` - update `texture_id: TextureId` field
   - `FixtureNode` - update `output_id: OutputId`, `texture_id: TextureId` fields
   - Any other references to node IDs

3. Update `ProjectRuntime` in `lp-core/src/project/runtime.rs`:
   - Change `HashMap<TextureId, ...>` to `HashMap<String, ...>` (or keep using ID types)
   - Update all ID usages

4. Update `ProjectConfig` in `lp-core/src/project/config.rs`:
   - Change `HashMap<u32, NodeType>` to `HashMap<String, NodeType>`
   - Update serialization/deserialization helpers

5. Update all code that creates or uses node IDs:
   - `ProjectBuilder`
   - `LpApp::create_default_project()`
   - Tests

## Success Criteria

- All node IDs changed to `String`
- All code updated to use string IDs
- Serialization/deserialization still works
- Code compiles without warnings
- Tests updated and passing

