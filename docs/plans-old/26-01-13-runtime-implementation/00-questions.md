# Questions: Runtime Implementation (Contexts, Texture, Fixture)

## Scope

Implementing the first three TODO items:

1. Runtime Contexts (NodeInitContext & RenderContext)
2. Texture Runtime (init + render)
3. Fixture Runtime (init + render)

## Questions

### Runtime Contexts

1. **Node Resolution**: ✅ **ANSWERED**

   - Have a common `resolve_node(spec: &NodeSpecifier) -> Result<NodeHandle, Error>` that looks up by path in `ProjectRuntime.nodes`
   - Type-specific methods (`resolve_output()`, `resolve_texture()`) delegate to `resolve_node()` then check kind
   - Error if wrong kind: `Error::WrongNodeKind { specifier: String, expected: NodeKind, actual: NodeKind }`
   - Return runtime-specific handles (`OutputHandle`/`TextureHandle`) that wrap `NodeHandle` internally

2. **Filesystem Access**: ✅ **ANSWERED**

   - Per-node filesystem using `LpFs::chroot()` to create a view rooted at the node directory
   - Read-only access (`&dyn LpFs`)
   - Node directory becomes the working directory (e.g., `/src/my.shader/` for shader node)
   - Paths like `main.glsl` resolve relative to node directory

3. **Texture Access**: ✅ **ANSWERED**

   - Lookup texture runtime by `TextureHandle` (which wraps `NodeHandle`)
   - Trigger lazy rendering automatically if texture not rendered this frame
   - Return `&Texture` reference (from `lp-shared::util::Texture`) instead of raw bytes
   - Lifetime tied to `RenderContext`
   - Store `Texture` instance in `TextureRuntime` so we can return reference
   - Note: `lp-shared::util::Texture` may need refactoring to avoid circular dependency with `lp-engine::error::Error`

4. **Output Access**: ✅ **ANSWERED**
   - Lookup output runtime by `OutputHandle` (which wraps `NodeHandle`)
   - Return `&mut [u8]` mutable slice to channel buffer for specified universe/channel range
   - Validate that `start_ch + ch_count` doesn't exceed available channels
   - Store channel buffer in `OutputRuntime` so we can return mutable slice
   - Handle case where output doesn't exist or isn't initialized

### Texture Runtime

5. **Texture Storage**: ✅ **ANSWERED**

   - Store `Texture` instance (from `lp-shared::util::Texture`) in `TextureRuntime`
   - `Texture` owns its `Vec<u8>` buffer internally
   - Use RGBA8 format initially (matches current `TextureState::texture_data`)
   - **Note**: `TextureState` may need reconsideration for efficient server/client sync of large texture buffers (defer for now)

6. **Texture Initialization**: ✅ **ANSWERED**

   - **Note**: `TextureConfig` should be a struct with `width`, `height`, `format` (not an enum)
   - Future: optional `file` property to initialize from file (defer with `todo!()`)
   - For now: allocate `Texture` with config dimensions and format, initialized to zeros
   - Store the `Texture` instance in `TextureRuntime`
   - Set status to `Ok` on success, `InitError` on failure

7. **Texture Rendering**: ✅ **ANSWERED**

   - Textures don't render themselves - shaders render TO textures
   - `TextureRuntime::render()` should be a no-op (or not exist)
   - When texture is requested via `get_texture()`, if `state_ver < frame_id`, shaders targeting that texture are run
   - Shader runtime's `render()` generates texture data
   - Texture's `state_ver` is updated to current frame after shaders run

8. **State Updates**: ✅ **ANSWERED**
   - Lazy rendering: When fixture requests texture via `get_texture()`, check if `state_ver < frame_id`
   - If texture needs rendering, find shaders targeting this texture and run them
   - Update texture's `state_ver` to current frame after shaders complete
   - This logic could be in `TextureRuntime::ensure_rendered()` or in `ProjectRuntime` - TextureRuntime makes sense
   - After `init()`: set `state_ver` to current frame (texture is initialized/ready)

### Fixture Runtime

9. **Handle Storage**: ✅ **ANSWERED**

   - Store `TextureHandle` and `OutputHandle` in `FixtureRuntime` after resolving in `init()`
   - Avoids re-resolving on every render
   - If resolution fails in `init()`, set status to `InitError` and don't create runtime
   - Store handles as fields in `FixtureRuntime` struct

10. **Texture Sampling**: ✅ **ANSWERED**

- Copy approach from old implementation: `/Users/yona/dev/photomancer/lp-cranelift/lp-app/crates/lp-engine/src.bk/nodes/fixture/runtime.rs`
- Use `SamplingKernel` with precomputed sample points in a circle (5x5 grid, Gaussian-like weights)
- Sample texture using `Texture::get_pixel()` at kernel positions around center points
- Use normalized UV coordinates (0-1 range), convert to pixel coordinates for sampling
- Weighted averaging of samples, normalize weights to sum to 1.0
- Transform matrix applied to coordinates (from fixture config)

11. **Output Writing**: ✅ **ANSWERED**

- Direct write to mutable buffer slice from `RenderContext::get_output()`
- Handle channel ordering (RGB, RGBA, etc.) based on fixture config
- Write to specific channel offsets based on mapping
- Handle channel count mismatches gracefully (write what fits, skip rest)
- Output's `state_ver` is updated when buffer is accessed (handled by `RenderContext`)

12. **Transform Matrix**: ✅ **ANSWERED**

- Fixture coordinate space is a unit square [-1, -1] to [1, 1] by default
- Affine transform maps from fixture coordinate space to texture UV space (0-1)
- Transform texture coordinates: apply 4x4 matrix to map fixture space to texture UV space
- For each mapping point, transform its coordinates using the matrix before sampling
- Allows users to position/move the fixture within the texture
