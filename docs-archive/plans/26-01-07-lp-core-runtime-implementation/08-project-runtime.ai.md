# Phase 8: ProjectRuntime

## Goal

Implement ProjectRuntime that manages the lifecycle of all nodes and coordinates updates.

## Tasks

1. Update `project/runtime.rs` with:
   - `ProjectRuntime` struct:
     - `uid: String`
     - `frame_time: FrameTime`
     - `textures: HashMap<TextureId, TextureNodeRuntime>`
     - `shaders: HashMap<ShaderId, ShaderNodeRuntime>`
     - `fixtures: HashMap<FixtureId, FixtureNodeRuntime>`
     - `outputs: HashMap<OutputId, OutputNodeRuntime>`
   - `new(uid: String) -> Self` constructor
   - `init(&mut self, config: &ProjectConfig, output_provider: &dyn OutputProvider) -> Result<(), Error>`:
     - Initializes nodes in order: textures → shaders → fixtures → outputs
     - Allows partial failures (nodes handle their own failures)
     - Creates `InitContext` for each node
   - `update(&mut self, delta_ms: u32, output_provider: &dyn OutputProvider) -> Result<(), Error>`:
     - Updates `frame_time.total_ms += delta_ms`, sets `frame_time.delta_ms = delta_ms`
     - Updates nodes in hard-coded order: shaders → fixtures → outputs
     - Creates appropriate type-specific contexts for each node:
       - Shaders: `ShaderRenderContext` with mutable texture access
       - Fixtures: `FixtureRenderContext` with read-only texture and mutable output access
       - Outputs: `OutputRenderContext` with no other node access
   - `destroy(&mut self) -> Result<(), Error>`:
     - Calls `destroy()` on all nodes in reverse order: outputs → fixtures → shaders → textures
   - `get_runtime_nodes(&self) -> RuntimeNodes`:
     - Derives `RuntimeNodes` from runtime instances for serialization
     - Converts type-safe IDs to u32 for serialization
   - `set_status(&mut self, node_type: NodeType, node_id: u32, status: NodeStatus)`
   - `get_status(&self, node_type: NodeType, node_id: u32) -> Option<&NodeStatus>`

2. Update `RuntimeNodes` and `NodeType` enum if needed (for serialization)

3. Add tests:
   - Test ProjectRuntime::new() creates empty runtime
   - Test init() initializes all nodes in correct order
   - Test init() handles partial failures gracefully
   - Test update() updates nodes in correct order
   - Test update() updates frame_time correctly
   - Test destroy() cleans up all nodes
   - Test get_runtime_nodes() serialization

## Success Criteria

- ProjectRuntime compiles and works correctly
- Node initialization order is correct
- Node update order is correct
- Frame time tracking works correctly
- Partial failures are handled gracefully
- All tests pass
- No warnings (except unused code that will be used in later phases)
- Code follows existing style

