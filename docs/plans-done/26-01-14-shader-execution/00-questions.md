# Questions: Shader Execution

## Scope

Implementing shader runtime execution:

1. Shader Runtime (init + render)
2. Lazy texture rendering integration
3. Shader state extraction

## Questions

### Shader Runtime

1. **GLSL Source Loading**: ✅ **ANSWERED**

   - Load GLSL source from node filesystem using `InitContext::get_node_fs()`
   - Path is relative to node directory (e.g., `main.glsl` resolves to `/src/my.shader/main.glsl`)
   - Store source code in `ShaderRuntime` for state extraction
   - Error if file not found or read fails

2. **Shader Compilation**: ✅ **ANSWERED**

   - Use `lp_glsl_compiler::GlslCompiler` to compile GLSL source
   - Compile to `GlslJitModule` (JIT executable)
   - Store compiled executable in `ShaderRuntime`
   - Capture compilation errors and store in state
   - Compile during `init()`, not during `render()`

3. **Shader Executable Storage**: ✅ **ANSWERED**

   - Store `GlslJitModule` (or `Box<dyn GlslExecutable>`) in `ShaderRuntime`
   - Need to handle lifetime: executable must outlive render calls
   - Store as `Option<Box<dyn GlslExecutable>>` to handle compilation failures

4. **Texture Handle Resolution**: ✅ **ANSWERED**

   - Resolve `texture_spec` from config using `InitContext::resolve_texture()`
   - Store `TextureHandle` in `ShaderRuntime` for render()
   - Error during init if texture not found or wrong kind

5. **Shader Execution**: ✅ **ANSWERED**

   - Execute shader's `main()` function for each pixel in texture
   - Signature: `vec4 main(vec2 fragCoord, vec2 outputSize, float time)`
   - Use `execute_main()` or `execute_function()` from `lp-glsl-compiler`
   - Write result (vec4 RGBA) to texture pixel
   - Get texture via `RenderContext::get_texture()` (mutable access needed)

6. **Time Parameter**: ✅ **ANSWERED**

   - Track frame time in `ProjectRuntime` with `delta_ms` and `total_ms` (like old engine)
   - `tick()` method should take `delta_ms: u32` parameter (from caller)
   - Update frame time: `total_ms += delta_ms`, `delta_ms = delta_ms`
   - Convert to seconds for shader: `time = total_ms as f32 / 1000.0`
   - Pass time as third parameter to shader main()
   - Reference: old engine had `FrameTime` struct and `tick(delta_ms: u32)` pattern

7. **Render Order**: ✅ **ANSWERED**

   - Shaders have `render_order` field in config (lower = render first)
   - When multiple shaders target same texture, execute in render_order
   - Sort shaders by render_order before execution
   - For now, assume one shader per texture (simpler)

### Lazy Texture Rendering

8. **Finding Shaders for Texture**: ✅ **ANSWERED**

   - Iterate through all shader nodes in `ProjectRuntime.nodes`
   - Check if shader's `texture_spec` resolves to the target texture handle
   - Collect matching shaders, sort by `render_order`
   - Execute shaders in order

9. **Shader Execution Order**: ✅ **ANSWERED**

   - Execute shaders in `render_order` (ascending: 0, 1, 2, ...)
   - Each shader writes to the same texture (overwrites previous)
   - Later shaders can use results from earlier shaders (if we add texture sampling later)
   - For now, just execute in order

10. **Texture Access During Shader Execution**: ✅ **ANSWERED**

    - Shaders need mutable access to texture to write pixels
    - `RenderContext::get_texture()` returns `&Texture` (immutable)
    - Need to add `get_texture_mut()` or change return type
    - Or: get texture handle, access texture runtime directly, get mutable reference
    - Better: add `get_texture_mut()` to `RenderContext` trait

11. **Error Handling**: ✅ **ANSWERED**

    - If shader compilation fails, store error in state, don't create executable
    - If shader execution fails, log error, continue with other shaders
    - Update shader node status to `Error` if execution fails
    - Don't fail entire texture rendering if one shader fails

### Shader State

12. **State Extraction**: ✅ **ANSWERED**

    - Store GLSL source code in `ShaderRuntime` (for sync API)
    - Store compilation error (if any) in `ShaderRuntime`
    - Extract state via `ShaderRuntime::get_state()` method
    - Return `ShaderState { glsl_code: String, error: Option<String> }`
    - Update `get_changes()` to extract actual state from `ShaderRuntime`
