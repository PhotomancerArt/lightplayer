# Overview: Runtime Implementation (Contexts, Texture, Fixture)

## Goal

Implement the first three TODO items from the engine core model:
1. Runtime Contexts (NodeInitContext & RenderContext)
2. Texture Runtime (init + render)
3. Fixture Runtime (init + render)

This will enable basic end-to-end rendering: textures are allocated and initialized, fixtures can sample textures and write to outputs, and the rendering pipeline works.

## Scope

- **Runtime Contexts**: Full implementation of `InitContext` and `RenderContextImpl` with node resolution, filesystem access, texture/output access, and lazy texture rendering
- **Texture Runtime**: Initialize textures from config, store `Texture` instances, extract state for sync API
- **Fixture Runtime**: Resolve dependencies, sample textures with kernel-based sampling, write to outputs with color ordering

## Out of Scope (for now)

- Shader runtime implementation (textures will be initialized but not rendered by shaders yet)
- Output runtime implementation (outputs will have buffers but not flush to hardware)
- Complex mapping types (using simplified mapping for now)
- Transform matrix application (basic implementation, full matrix math later)

## Success Criteria

- All code compiles
- Can initialize texture nodes with config
- Can initialize fixture nodes with resolved dependencies
- Can render fixtures that sample textures and write to outputs
- Lazy texture rendering works (triggers when texture is accessed)
- State extraction works for sync API
- All tests pass
