# Phase 6: Shader Node Runtime

## Goal

Implement ShaderNodeRuntime and ShaderNodeConfig, including GLSL compilation and execution.

## Tasks

1. Add `lp-glsl-compiler` dependency to `lp-core/Cargo.toml`:
   - `lp-glsl-compiler = { path = "../lp-glsl-compiler" }` (or workspace reference)

2. Create `nodes/shader/config.rs`:
   - Move `ShaderNode` enum from `nodes/shader.rs` to here
   - Rename to `ShaderNodeConfig` (or keep as `ShaderNode` if that's the pattern)
   - Ensure `texture_id` uses `TextureId` type
   - Export from `nodes/shader/mod.rs`

3. Create `nodes/shader/runtime.rs` with:
   - `ShaderNodeRuntime` struct:
     - `executable: Option<Box<dyn GlslExecutable>>`
     - `texture_id: TextureId`
     - `status: NodeStatus`
   - Implement `NodeLifecycle` trait:
     - `Config = ShaderNodeConfig`
     - `RenderContext = ShaderRenderContext`
     - `init()`: 
       - Validates GLSL has signature `vec4 main(vec2 fragCoord, vec2 outputSize, float time)`
       - Compiles GLSL using `lp-glsl-compiler` compiler
       - Stores executable (or None if compilation failed)
       - Sets status to Error if compilation fails
     - `update()`:
       - If executable is None, skip (return Ok)
       - Get texture via `ctx.get_texture_mut(texture_id)`
       - Iterate over all texture pixels
       - For each pixel: call shader with `[x, y]` as fragCoord, `[width, height]` as outputSize, `time.total_ms` as time
       - Write result via `texture.set_pixel()`
       - Handle errors (set status to Error)
     - `destroy()`: Cleanup executable if needed

4. Update `nodes/shader/mod.rs` to export config and runtime

5. Add tests:
   - Test ShaderNodeRuntime::init() with valid GLSL (compiles successfully)
   - Test ShaderNodeRuntime::init() with invalid GLSL (sets status to Error)
   - Test signature validation
   - Test update() executes shader and writes to texture
   - Test update() skips if executable is None

## Success Criteria

- ShaderNodeRuntime compiles and implements NodeLifecycle
- GLSL compilation works (using lp-glsl-compiler)
- Shader execution works correctly
- Pixel writing works correctly
- All tests pass
- No warnings (except unused code that will be used in later phases)
- Code follows existing style

