# Phase 4: Texture Node Runtime

## Goal

Implement TextureNodeRuntime and TextureNodeConfig, and refactor existing TextureNode to use config structure.

## Tasks

1. Create `nodes/texture/config.rs`:
   - Move `TextureNode` enum from `nodes/texture.rs` to here
   - Rename to `TextureNodeConfig` (or keep as `TextureNode` if that's the pattern)
   - Ensure it uses type-safe IDs where needed
   - Export from `nodes/texture/mod.rs`

2. Create `nodes/texture/runtime.rs` with:
   - `TextureNodeRuntime` struct:
     - `texture: Texture`
     - `status: NodeStatus`
   - Methods: `texture() -> &Texture`, `texture_mut() -> &mut Texture`
   - Implement `NodeLifecycle` trait:
     - `Config = TextureNodeConfig`
     - `RenderContext = TextureRenderContext`
     - `init()`: Creates texture via `Texture::new()` with config size and format
     - `update()`: No-op for now (textures don't update themselves)
     - `destroy()`: Cleanup if needed

3. Update `nodes/texture/mod.rs` to export config and runtime

4. Update `project/runtime.rs` to include `NodeStatus` enum (if not already present):
   - `Ok`
   - `Error { status_message: String }`

5. Add tests:
   - Test TextureNodeRuntime::init() creates texture correctly
   - Test texture() and texture_mut() accessors
   - Test status tracking (Ok initially, Error on init failure)
   - Test destroy() (if needed)

## Success Criteria

- TextureNodeRuntime compiles and implements NodeLifecycle
- Config structure is properly separated
- All tests pass
- No warnings (except unused code that will be used in later phases)
- Code follows existing style

