# Phase 9: ProjectBuilder

## Goal

Implement ProjectBuilder with fluent API for easy project setup and testing.

## Tasks

1. Create `builder.rs` with:
   - `ProjectBuilder` struct:
     - `uid: String`
     - `name: String`
     - `next_id: u32` (auto-incrementing ID generator)
     - `nodes: Nodes` (from project/config.rs)
   - `new() -> Self` constructor
   - `with_uid(&mut self, uid: String) -> &mut Self` (fluent)
   - `with_name(&mut self, name: String) -> &mut Self` (fluent)
   - `add_texture(&mut self, config: TextureNodeConfig) -> TextureId`:
     - Auto-generates ID, inserts into nodes.textures, returns ID
   - `add_shader(&mut self, texture_id: TextureId, config: ShaderNodeConfig) -> ShaderId`:
     - Auto-generates ID, inserts into nodes.shaders, returns ID
   - `add_output(&mut self, config: OutputNodeConfig) -> OutputId`:
     - Auto-generates ID, inserts into nodes.outputs, returns ID
   - `add_fixture(&mut self, output_id: OutputId, texture_id: TextureId, config: FixtureNodeConfig) -> FixtureId`:
     - Auto-generates ID, inserts into nodes.fixtures, returns ID
   - `build(self) -> Result<ProjectConfig, Error>`:
     - Validates project (checks that referenced IDs exist)
     - Returns `ProjectConfig` with all nodes

2. Export from `lib.rs`

3. Add tests:
   - Test ProjectBuilder::new() creates empty builder
   - Test fluent API (with_uid, with_name)
   - Test add methods return correct IDs
   - Test build() validates project correctly
   - Test build() returns error on invalid references
   - Test complete project building workflow

## Success Criteria

- ProjectBuilder compiles and works correctly
- Fluent API works correctly
- Auto-generated IDs work correctly
- Validation works correctly
- All tests pass
- No warnings (except unused code that will be used in later phases)
- Code follows existing style

