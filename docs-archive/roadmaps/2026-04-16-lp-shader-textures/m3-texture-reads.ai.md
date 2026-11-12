# M3 — Texture Reads

## Goal

Shaders can read from textures (palette lookups, data textures, multi-pass).
Adds a texture table to vmctx, a `texelFetch` builtin, and frontend support
for sampler types.

## Deliverables

### Texture table in vmctx

Extend vmctx layout with a texture descriptor table:

```
┌─────────────────────┐  offset 0
│   VmContext header   │
├─────────────────────┤
│   Uniforms           │
├─────────────────────┤
│   Globals            │
├─────────────────────┤
│   Outputs            │
├─────────────────────┤
│   Texture table      │  NEW: array of TextureDescriptor
├─────────────────────┤
│   Globals snapshot   │
└─────────────────────┘
```

Each `TextureDescriptor`:
```rust
#[repr(C)]
struct TextureDescriptor {
    base_ptr: u32,     // pointer to pixel data (guest address)
    width: u32,
    height: u32,
    format: u32,       // TextureStorageFormat as u32
    stride: u32,       // bytes per row
}
```

### `texelFetch` builtin

Integer-coordinate texture lookup. Registered as an lpfn-style builtin:

```glsl
vec4 texelFetch(sampler2D tex, ivec2 coord, int lod);
// lod is ignored (always 0) for CPU path
```

Lowered to an import call that:
1. Reads the texture descriptor from vmctx (base_ptr, width, format)
2. Bounds-checks coordinates
3. Computes byte offset based on format
4. Loads pixel data, converts to float (unorm16 -> f32: `value / 65535.0`)
5. Returns vec4 (RGB formats return 1.0 for alpha)

The builtin implementation lives in `lps-builtins` (like existing math
builtins), with format-specific fast paths.

### `lps-frontend`: sampler2D support

- Map naga's `ImageClass` / `SamplerType` types to a new `LpsType::Sampler2D`
  (or handle via the texture table index as a uniform int).
- `uniform sampler2D myTexture;` maps to a texture table slot index.
- The runtime binds textures by setting the slot index uniform.

### `lp-shader` API

```rust
impl<I: LpvmInstance> FragInstance<I> {
    /// Bind a texture to a sampler slot.
    pub fn bind_texture(
        &mut self,
        name: &str,
        texture: &dyn TextureBuffer,
    ) -> Result<(), Error>;
}
```

### `texture()` with normalized coordinates (stretch goal)

```glsl
vec4 texture(sampler2D tex, vec2 uv);
```

Normalized [0,1] coordinates with bilinear filtering. More complex than
`texelFetch` but important for real effects. May be deferred to a follow-up.

## Validation

```bash
cargo test -p lp-shader
# Test: shader reads from a data texture via texelFetch
# Test: palette lookup effect
```

## Dependencies

- M2 (render_frame, FragInstance)
- Texture buffer types from M0
