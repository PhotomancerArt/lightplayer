# Phase 4: Implement node type definitions

## Goal

Extract node type definitions into separate modules for better organization.

## Tasks

1. Create `src/nodes/output.rs`:
   - Move `OutputNode` enum from `project/config.rs`
   - Add any output-specific utilities
2. Create `src/nodes/texture.rs`:
   - Move `TextureNode` enum from `project/config.rs`
   - Add texture format constants/utilities (RGB8, RGBA8, R8)
   - Add format validation
3. Create `src/nodes/shader.rs`:
   - Move `ShaderNode` enum from `project/config.rs`
   - Add shader-specific utilities
4. Create `src/nodes/fixture.rs`:
   - Move `FixtureNode` enum and `Mapping` struct from `project/config.rs`
   - Add fixture-specific utilities
5. Update `src/nodes/mod.rs` to export all node types
6. Update `src/project/config.rs` to use node types from `nodes` module

## Success Criteria

- All node types are properly organized in separate modules
- Node types are exported correctly
- `ProjectConfig` still compiles and works
- All code compiles without warnings

