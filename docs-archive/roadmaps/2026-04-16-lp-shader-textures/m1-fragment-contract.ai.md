# M1 — Fragment Shader Contract

## Goal

Formalize fragment shaders in lps-frontend and lp-shader. Shaders use
standard GLSL fragment output (`out vec4 fragColor; void main()`) instead of
the current `vec4 render(vec2, vec2, float)` function-argument convention.

## Deliverables

### `lps-frontend`: fragment shader parsing

Switch from `ShaderStage::Vertex` to `ShaderStage::Fragment` in
`parse_glsl()`. Handle naga's representation of fragment outputs:

- `AddressSpace::Output` globals (e.g. `out vec4 fragColor`) -- lower to
  stores into the vmctx outputs region, analogous to how `Private` globals
  are lowered today.
- `BuiltIn::Position` on fragment entry point inputs -- this is
  `gl_FragCoord`. Map to a runtime-injected uniform that the pixel loop
  sets per-pixel.

### `lps-shared`: output type metadata

Add `outputs_type: Option<LpsType>` to `LpsModuleSig` (alongside existing
`uniforms_type` and `globals_type`). This describes the fragment output
layout in vmctx so the runtime knows where to read the pixel color after
calling `main()`.

### VMContext layout update

```
┌─────────────────────┐  offset 0
│   VmContext header   │
├─────────────────────┤  header_size
│   Uniforms region    │  host-writable, shader read-only
├─────────────────────┤
│   Globals region     │  shader read-write
├─────────────────────┤
│   Outputs region     │  NEW: shader-writable, host-readable
├─────────────────────┤
│   Globals snapshot   │
└─────────────────────┘
```

The outputs region holds fragment outputs (`out vec4 fragColor` etc.).
The runtime reads from known offsets after `main()` returns.

### `lp-shader`: `compile_frag`

```rust
pub struct FragOutputDesc {
    pub storage_format: TextureStorageFormat,
}

impl<E: LpvmEngine> LpsShaderEngine<E> {
    pub fn compile_frag(
        &self,
        glsl: &str,
        output: &FragOutputDesc,
    ) -> Result<FragModule<E::Module>, Error>;
}

pub struct FragModule<M: LpvmModule> {
    module: M,
    meta: LpsModuleSig,
    output_desc: FragOutputDesc,
    // offset + type of fragColor in vmctx outputs region
    frag_color_offset: u32,
    frag_color_components: u32,  // 3 for vec3, 4 for vec4
}
```

### Runtime uniforms

`gl_FragCoord` (vec2) and `outputSize` (vec2) are injected as uniforms by
`compile_frag`. The caller never declares them -- the compiler prepends
them to the uniform block. `time` remains a user-declared uniform.

### Bootstrap wrapper for legacy `render()` functions

`lp-shader` (or lpfx) can wrap old-style `render(fragCoord, outputSize, time)`
into the new contract:

```glsl
// prepended by compile_frag when it detects render() but no main()
uniform float time;
out vec4 fragColor;
void main() { fragColor = render(gl_FragCoord.xy, outputSize, time); }
```

This preserves backward compatibility for existing shaders.

## Validation

```bash
cargo test -p lps-frontend   # existing + new fragment tests
cargo test -p lp-shader      # compile_frag smoke tests
```

Existing filetests may need review -- switching ShaderStage may affect naga
parsing behavior.

## Risks

- Naga's `ShaderStage::Fragment` may parse GLSL differently (e.g. requiring
  `main` to have specific signature). Needs investigation during implementation.
- Existing shaders that don't declare `out` variables need the bootstrap
  wrapper or a graceful fallback.

## Dependencies

- M0 (lp-shader crate exists, TextureStorageFormat defined)
