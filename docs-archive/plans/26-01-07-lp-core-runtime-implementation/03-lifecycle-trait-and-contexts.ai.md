# Phase 3: Lifecycle Trait and Contexts

## Goal

Implement the NodeLifecycle trait and all render contexts that provide type-safe access to runtime data.

## Tasks

1. Create `runtime/lifecycle.rs` with:
   - `NodeLifecycle` trait with:
     - Associated types: `Config` and `RenderContext`
     - `init(&mut self, config: &Self::Config, ctx: &InitContext) -> Result<(), Error>`
     - `update(&mut self, ctx: &Self::RenderContext) -> Result<(), Error>`
     - `destroy(&mut self) -> Result<(), Error>`

2. Create `runtime/contexts.rs` with:
   - `InitContext<'a>` struct:
     - `project_config: &'a ProjectConfig`
     - Methods: `get_texture_config(id: TextureId) -> Option<&TextureNodeConfig>`
     - Methods: `get_shader_config(id: ShaderId) -> Option<&ShaderNodeConfig>`
     - Methods: `get_fixture_config(id: FixtureId) -> Option<&FixtureNodeConfig>`
     - Methods: `get_output_config(id: OutputId) -> Option<&OutputNodeConfig>`
   - `ShaderRenderContext<'a>` struct:
     - `time: FrameTime`
     - `textures: &'a mut HashMap<TextureId, TextureNodeRuntime>`
     - Method: `get_texture_mut(texture_id: TextureId) -> Option<&mut Texture>`
   - `FixtureRenderContext<'a>` struct:
     - `time: FrameTime`
     - `textures: &'a HashMap<TextureId, TextureNodeRuntime>`
     - `outputs: &'a mut HashMap<OutputId, OutputNodeRuntime>`
     - Method: `get_texture(texture_id: TextureId) -> Option<&Texture>`
     - Method: `get_output_mut(output_id: OutputId) -> Option<&mut OutputNodeRuntime>`
   - `OutputRenderContext` struct:
     - `time: FrameTime`
   - `TextureRenderContext` struct:
     - `time: FrameTime`

3. Update `runtime/mod.rs` to export all modules

4. Add necessary imports (use type-safe IDs, FrameTime, etc.)

5. Add tests:
   - Test InitContext methods (get configs by ID)
   - Test render context creation and field access
   - Test render context methods (get_texture_mut, get_texture, get_output_mut)

## Success Criteria

- NodeLifecycle trait compiles
- All context types compile
- Context methods work correctly
- All tests pass
- No warnings (except unused code that will be used in later phases)
- Code follows existing style

