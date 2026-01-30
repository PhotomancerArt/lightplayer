# Overview: Shader Execution

## Goal

Implement shader runtime execution so that shaders can compile GLSL code and render to textures. This completes the rendering pipeline: shaders render to textures, fixtures sample textures and write to outputs.

## Scope

- **Shader Runtime**: Load GLSL source from filesystem, compile using `lp-glsl-compiler`, store compiled executable, execute per-pixel to render to textures
- **Lazy Texture Rendering**: Update `ensure_texture_rendered()` to find shaders targeting a texture, execute them in render order, write results to texture
- **Shader State**: Extract shader state (GLSL code, compilation errors) for sync API
- **Time Parameter**: Provide frame time to shaders (from `ProjectRuntime.frame_id` or separate time tracking)

## Out of Scope (for now)

- Shader input textures (sampling other textures in shaders)
- Uniforms/parameters (shader configuration beyond time)
- Shader caching/recompilation (recompile on every init for now)
- Multiple shaders per texture (only one shader per texture for now, or execute all in order)

## Success Criteria

- Shaders can load GLSL source from node filesystem
- Shaders compile successfully using `lp-glsl-compiler`
- Shaders execute per-pixel to render to textures
- Lazy texture rendering triggers shader execution
- Shader errors are captured and reported via state
- All tests pass

## Dependencies

- `lp-glsl-compiler` crate (already a dependency)
- Runtime contexts (already implemented)
- Texture runtime (already implemented)
- Node filesystem access (already implemented via `InitContext`)
