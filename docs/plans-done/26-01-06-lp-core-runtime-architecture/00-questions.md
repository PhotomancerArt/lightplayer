# Questions: LP-Core Runtime Architecture

## Current State

Currently, `lp-core` has:

- `ProjectConfig` - static configuration (JSON-serializable)
- `ProjectRuntime` - minimal runtime state (just status tracking)
- Node types (`TextureNode`, `ShaderNode`, `FixtureNode`, `OutputNode`) - these are configs, not
  runtime instances
- No lifecycle management for nodes
- No rendering context or frame timing
- No project builder for tests

## Proposed Changes

1. **Config vs Runtime Separation**: Rename node types to `*NodeConfig` and create corresponding
   `*NodeRuntime` types
2. **Node Lifecycle**: Each node type needs `init()`, `render()`, `destroy()` methods
3. **Project Runtime**: Enhanced runtime with lifecycle and state management
4. **Render Context**: Add `RenderContext` with frame timing (delta_t, absolute_time)
5. **No Internal Loop**: Runtime is driven by firmware, not self-contained
6. **Project Builder**: Helper for constructing test projects

## Questions

1. **Node Lifecycle Trait**: ✅ **DECIDED**: Trait-based approach. Nodes need typed access to their
   config type (associated types/generics). Lifecycle methods use `&mut self`. `render()` does not
   return data - instead, we need node-kind-specific traits for data access (e.g.,
   `TextureNodeRuntime` implements a trait for reading texture data, `OutputNodeRuntime` for writing
   LED data).

2. **Runtime State Storage**: ✅ **DECIDED**: State lives in the `*NodeRuntime` structs. Need raw
   access to contiguous memory (Vec might work, but may need direct alloc calls). Texture helper
   will eventually move to `lp-glsl-builtins`. `ProjectRuntime` uses separate
   `HashMap<u32, *NodeRuntime>` per node type. Runtime must provide access to other nodes during
   rendering.

3. **Render Context Contents**: ✅ **DECIDED**: Use contexts instead of passing runtime directly (
   helps with testing). `RenderContext` contains `delta_t` and `absolute_time`, plus methods to
   access other nodes (`get_texture(id: TextureId)`, etc.). Use type-specific ID wrappers (
   `TextureId`, `OutputId`, `ShaderId`, `FixtureId`) for type safety.

4. **Error Handling**: ✅ **DECIDED**: Remove `Warn` status, use only `Ok`/`Error`. Lifecycle methods
   return `Result<(), Error>`. If `update()` (renamed from `render()` for generality) returns error,
   update `NodeStatus` to `Error` with the error message.

5. **Node Dependencies**: ✅ **DECIDED**: No dependency tracking for now. Hard-coded update order:
   shaders → fixtures → output devices. Getting node values returns current state (no lazy
   rendering). Dependencies validated at init time using `InitContext`.

6. **Texture Data**: ✅ **DECIDED**: Create `Texture` abstraction with metadata (size, format) and
   `u8` pointer/Vec for data. Fixed size (not resizable). Associated inline methods for speed.
   Helper methods like `compute_all` to abstract math. Store in `TextureNodeRuntime`. Shaders access
   through `RenderContext.get_texture(id)`.

7. **Shader Compilation**: ✅ **DECIDED**: Compile in `init()` (returns `Result`). Cache
   `JitExecutable` in `ShaderNodeRuntime`. On compilation error, set `Executable` to `None` and skip
   rendering. Per-node updates (recompilation) will be handled later, not now.

8. **Fixture Mapping**: ✅ **DECIDED**: Fixtures sample textures in `update()` and directly write
   channel values to output buffers. Access textures via `RenderContext.get_texture(id)`. Precompute
   sampling kernels in `init()`. Fixture config should have `texture_id` (check if it exists, add if
   not).

9. **Output Rendering**: ✅ **DECIDED**: `OutputNodeRuntime` has `update()` that delegates to
   firmware-specific LED output interface. `init()` provisions/gets a handle to LED output.
   Firmware-specific code handles actual hardware writes. Single buffer, no clearing - persists
   between frames (if fixture doesn't update, stays same as last frame). Fixtures write via
   `output_id`.

10. **Project Builder API**: ✅ **DECIDED**: Fluent API. Validate at `build()` time. Auto-generate
    IDs. Need to handle linking (e.g., shader needs `texture_id`, fixture needs `output_id` and
    `texture_id`) - methods return IDs that can be used for linking. `build()` returns `Result`.

11. **Runtime Initialization Order**: ✅ **DECIDED**: Order: textures → shaders → fixtures → outputs.
    Partial failures allowed - nodes handle their own failures. Cascading failures handled by
    returning `Option` (e.g., `get_texture()` returns `Option`, fixture fails if texture not
    available). All nodes start in default state - project can fully `init()` even with failures,
    but may not work. Allow partial success.

12. **Frame Timing**: ✅ **DECIDED**: Simple timing with `delta_ms` and `total_ms` (milliseconds).
    Top-level render function takes `delta_ms` as parameter. Add `delta_ms` to internal `total_ms` (
    no clamping). Names reflect type (delta_ms, total_ms).

