# Design Issues Analysis

## Critical Issues

### 1. Shader Writing to Textures
**Problem**: `ShaderRenderContext.get_texture()` returns `Option<&Texture>` (immutable), but shaders need to WRITE to textures.

**Current Design**:
```
ShaderRenderContext {
  textures: &'a HashMap<TextureId, TextureNodeRuntime>,
}
Methods: get_texture(texture_id: TextureId) -> Option<&Texture>
```

**Issue**: Shaders render TO textures, so they need mutable access.

**Solution Options**:
- Option A: `get_texture_mut()` returns `Option<&mut Texture>` - but this conflicts with immutable borrow of HashMap
- Option B: Shaders return rendered data, ProjectRuntime writes to textures
- Option C: `ShaderRenderContext` has mutable access to textures HashMap

### 2. FixtureRenderContext Borrow Checker ✅ NOT AN ISSUE
**Problem**: Initially thought `FixtureRenderContext` borrow checker issue when calling `fixture.update(&mut fixture, ctx)`.

**Resolution**: Not an issue. We're not passing the fixtures HashMap to fixtures - only textures and outputs. These are different fields in `ProjectRuntime`, so Rust allows both borrows. No conflict.

### 3. JitExecutable Type Mismatch ✅ FIXED
**Problem**: Design mentioned `JitExecutable` but codebase has `GlslJitModule` and `GlslExecutable` trait.

**Solution**: Changed to `Option<Box<dyn GlslExecutable>>`. `GlslJitModule` implements `GlslExecutable` trait, allowing flexibility for different backends.

### 4. Texture Extraction from TextureNodeRuntime ✅ FIXED
**Problem**: `ShaderRenderContext` stores `HashMap<TextureId, TextureNodeRuntime>`, but `get_texture_mut()` returns `&mut Texture`. How do we extract Texture from TextureNodeRuntime?

**Solution**: Added `texture()` and `texture_mut()` methods on `TextureNodeRuntime`. `ShaderRenderContext::get_texture_mut()` uses `textures.get_mut().map(|rt| rt.texture_mut())`.

## Design Gaps

### 5. SamplingKernel Definition Missing ✅ FIXED
**Problem**: Design mentioned `Vec<SamplingKernel>` but didn't define the struct.

**Solution**: Changed to single `SamplingKernel` per fixture (not Vec). Kernel has `radius` (normalized, same for all pixels) and `samples: Vec<SamplePoint>` with relative offsets and weights. One kernel is precomputed in `init()` and reused for all mapping points at their respective centers.

### 6. OutputProvider in update() ✅ FIXED
**Problem**: Design showed `update(delta_ms, output_provider: &mut dyn OutputProvider)` but outputs are already initialized.

**Solution**: Removed `OutputProvider` from `update()`. Outputs are created in `init()`, so `update()` only needs `delta_ms`.

### 7. NodeStatus Synchronization ✅ FIXED
**Problem**: Current `ProjectRuntime` had separate `RuntimeNodes` for serialization, but new design has `status` in each runtime struct.

**Solution**: Removed separate `RuntimeNodes` field. Runtime instances are the source of truth. `get_runtime_nodes()` derives `RuntimeNodes` from runtime instances when needed for serialization. `set_status()` and `get_status()` operate directly on runtime instances.

### 8. Texture Format Handling ✅ FIXED
**Problem**: Design didn't specify how shaders know texture format when writing, or how fixtures know format when reading.

**Solution**: Shaders always return vec4 (RGBA). `Texture::set_pixel()` writes based on format: RGB8 writes first 3 bytes, R8 writes first byte, RGBA8 writes all 4 bytes. `Texture` provides `format()` and `bytes_per_pixel()` methods for querying format. Fixtures can query format when sampling.

### 9. Error Cascading ✅ FIXED
**Problem**: If a shader fails to render (executable is None), what happens to fixtures that depend on that texture?

**Solution**: Graceful degradation. If `get_texture()` returns `None` (texture missing or failed), fixture's `update()` returns `Err` and sets status to `Error`. Output buffer keeps previous frame's values (as per design: "if fixture doesn't update, stays same as last frame"). Project continues running with partial failures.

### 10. OutputNodeRuntime.update() Purpose ✅ FIXED
**Problem**: Design showed `OutputNodeRuntime` has `update()` method, but wasn't clear what it does.

**Solution**: `OutputNodeRuntime` has a pixel buffer that fixtures write to. `update()` reads the buffer and calls `handle.write_pixels()` to send to hardware (ESP32) or update UI (host). This is why we might need to pass output provider to update() - but actually the handle is stored in the runtime, so it should work. Buffer persists between frames if not updated.

### 11. BaseRenderContext vs Individual Contexts ✅ FIXED
**Problem**: Design showed `BaseRenderContext` with timing, but individual contexts had `base: BaseRenderContext`, requiring `ctx.base.delta_ms` access.

**Solution**: Use `Time` struct with `delta_ms` and `total_ms` fields. Each context has `time: Time` field. Access via `ctx.time.delta_ms` and `ctx.time.total_ms`.

### 12. ProjectRuntime.config Field ✅ FIXED
**Problem**: Design showed `config: Option<ProjectConfig>` in `ProjectRuntime`, but it's only needed during `init()`.

**Solution**: Removed `config` field. Config is passed to `init()` but not stored. If re-initialization is needed, caller passes config again.

## Minor Issues

### 13. Builder API Return Types ✅ FIXED
**Problem**: Builder methods returned `(Self, Id)` tuples, making chaining awkward.

**Solution**: Changed to `&mut self` methods that return just the ID. Allows: `let tex_id = builder.add_texture(...); let shader_id = builder.add_shader(tex_id, ...);`

### 14. Texture in util/ vs nodes/texture/
**Problem**: `Texture` is in `util/texture.rs` but `TextureNodeRuntime` uses it. Should there be a relationship or import?

**Clarification**: This seems fine, just noting the separation.

