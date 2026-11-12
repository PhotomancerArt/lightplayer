# M1 — Pixel Shader Contract: Design

## Scope

Define the pixel shader contract in `lp-shader` and validate it at compile
time. Rename M0's fragment shader types to pixel shader types. No frontend
changes.

A pixel shader is a GLSL function named `render` that takes pixel coordinates
and returns a color:

```glsl
uniform float time;
uniform vec2 outputSize;

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    return vec4(uv, sin(time), 1.0);
}
```

`pos` is pixel coordinates (like `gl_FragCoord.xy`): `(0.5, 0.5)` to
`(width - 0.5, height - 0.5)`. Return type must match the output format's
channel count.

## File Structure

```
lp-shader/lp-shader/src/
├── lib.rs                          # UPDATE: re-exports (Frag → Px)
├── engine.rs                       # UPDATE: compile_frag → compile_px + validation
├── frag_shader.rs → px_shader.rs   # RENAME + UPDATE: LpsPxShader
├── error.rs                        # UPDATE: add validation error variant
├── texture_buf.rs                  # NO CHANGE
└── tests.rs                        # UPDATE: pixel shader tests
```

## Conceptual Architecture

```
GLSL source (pixel shader convention)
    │
    │  "vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }"
    │
    ▼
lps_frontend::compile()          ← UNCHANGED (vertex stage)
lps_frontend::lower()            ← UNCHANGED
    │
    ▼
LpirModule + LpsModuleSig
    │  functions: [{name: "render", params: [vec2], return: vec4}]
    │
    ▼
LpsEngine::compile_px(glsl, Rgba16Unorm)
    │  1. compile + lower (existing path)
    │  2. find "render" in LpsModuleSig.functions
    │  3. validate signature vs output format
    │  4. compile LPIR → backend module
    │  5. instantiate → LpsPxShader
    │
    ▼
LpsPxShader<M>
    │  module, instance, meta, output_format, render_fn_index
    │
    ▼
(M2: render_frame pixel loop)
```

## Key Types

### `LpsPxShader<M: LpvmModule>`

Renamed from `LpsFragShader`. Holds compiled module + instance + metadata.

```rust
pub struct LpsPxShader<M: LpvmModule> {
    #[allow(dead_code, reason = "retain compiled module for instance lifetime")]
    module: M,
    instance: RefCell<M::Instance>,
    output_format: TextureStorageFormat,
    meta: LpsModuleSig,
    render_fn_index: usize,
}
```

`render_fn_index` is the index of the `render` function in
`meta.functions`. Resolved once at construction time.

### `LpsEngine::compile_px`

```rust
pub fn compile_px(
    &self,
    glsl: &str,
    output_format: TextureStorageFormat,
) -> Result<LpsPxShader<E::Module>, LpsError>
```

Validation after compile + lower:
1. Find `render` in `LpsModuleSig.functions` → error if missing
2. Check param count == 1, param type == `Vec2` → error if wrong
3. Check return type matches format channel count:
   - `Rgba16Unorm` → `Vec4`
   - (future formats follow same pattern)
4. Check return type is float-based → error if int/uint/bool

### `LpsError` additions

```rust
pub enum LpsError {
    Parse(String),
    Lower(String),
    Compile(String),
    Render(String),
    /// Pixel shader contract validation failure.
    Validation(String),
}
```

## Design Decisions

### Why not GLSL fragment shaders

Naga's `ShaderStage::Fragment` wraps `out vec4 fragColor; void main()` into
a complex entry point with built-in args, private globals, and a return
struct. This adds indirection that hurts CPU codegen and inlining.

The `render(vec2 pos) -> vecN` convention:
- Uses the existing vertex-stage `compile()` path (no frontend changes)
- Produces clean LPIR: `func @render(f32, f32) -> (f32, f32, f32, f32)`
- Return value = pixel color (no vmctx outputs region)
- Args = pixel position (no vmctx built-in uniforms)
- Best codegen and inlining for future `__render_frame`

### Return type tied to output format

Hard error if mismatch. `Rgba16Unorm` requires `vec4 render(vec2)`.
Future `Rgb16Unorm` would require `vec3`, `R16Unorm` would require `float`.

### No changes to lps-frontend or LpsModuleSig

The pixel shader contract is enforced by `lp-shader::compile_px`. The
frontend stays a general-purpose GLSL→LPIR compiler. `LpsModuleSig` stays
a neutral metadata type.

### No bootstrap wrapper

No legacy `render(fragCoord, outputSize, time)` support. No public
consumers exist. Clean break.

### Uniforms are user-declared

`outputSize`, `time`, etc. are regular `uniform` declarations. The runtime
sets them if present. No auto-injection.
