# M1 — Pixel Shader Contract: Planning Notes

## Scope of Work

Define the pixel shader contract in `lp-shader` and validate it at compile
time. A pixel shader is a GLSL function named `render` that takes pixel
coordinates and returns a color:

```glsl
uniform float time;
uniform vec2 outputSize;

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    return vec4(uv, sin(time), 1.0);
}
```

This is NOT a GLSL fragment shader. No `ShaderStage::Fragment`, no entry
points, no `gl_FragCoord`, no `out` variables. It's a regular GLSL function
that the existing `compile()` path handles. `lp-shader` validates the
signature and records the metadata.

## Key Design Decisions

### Pixel shader, not fragment shader

Naga's fragment shader model wraps `out vec4 fragColor; void main()` into
a complex entry point with built-in args, private globals, and a return
struct. This adds indirection that hurts CPU codegen and inlining.

Instead: `render(vec2 pos) -> vecN` is a regular function. Clean LPIR:
```
func @render(f32, f32) -> (f32, f32, f32, f32)
```

Benefits:
- No new frontend path needed (`compile()` as-is)
- No entry point or fragment stage handling
- Return type tied to output format (validated at compile time)
- Best codegen: args/returns in registers, no vmctx indirection
- Best inlining: `__render_frame` + `render` fuse into one flat function
- GPU path (lpfx) wraps with its own `main()` shim when needed

### Return type matches output format

- `Rgba16Unorm` → `vec4 render(vec2 pos)`
- Future `Rgb16Unorm` → `vec3 render(vec2 pos)` (saves 2KB on 32x32)
- Future `R16Unorm` → `float render(vec2 pos)` (data textures)

`compile_pixel` hard-errors if the return type doesn't match the format.
No silent truncation or padding.

### `vec2 pos` = pixel coordinates

Like `gl_FragCoord.xy`: ranges from `(0.5, 0.5)` to
`(width - 0.5, height - 0.5)`. Standard GPU convention. Shader divides
by `outputSize` to get normalized UV.

### Uniforms are user-declared

`outputSize`, `time`, etc. are regular `uniform` declarations in the GLSL
source. The runtime sets them if present. No auto-injection.

### No bootstrap wrapper

No legacy `render(fragCoord, outputSize, time)` support. No public API
consumers exist. All previous shader conventions were POC. This is the
correct API.

### Existing `compile()` unchanged

696 filetests use `compile()` with vertex stage. It stays as-is — still
valuable for testing the underlying compilation pipeline. `compile_pixel`
calls `compile()` + `lower()` then validates the `render` signature.

## Current State of Codebase

### `lps-frontend/src/parse.rs`

- `compile()` → `prepared_glsl_for_compile()` → `parse_glsl(Vertex)` →
  `naga_module_from_parsed()` → `NagaModule` with helper functions
- `ensure_vertex_entry_point()` appends `void main() {}` when missing
- This path works for pixel shaders — `render(vec2)` is a regular function

### `lps-frontend/src/naga_types.rs`

- `extract_functions()` walks `module.functions`, skips `main` when empty
- `render(vec2 pos) -> vec4` will be extracted normally with correct
  `FunctionInfo` (params, return type)

### `lps-frontend/src/lower.rs`

- `lower()` produces `(LpirModule, LpsModuleSig)`
- `LpsModuleSig.functions` has `LpsFnSig` for each function
- The `render` function's signature will be in `functions` — params and
  return type match the GLSL declaration

### `lp-shader/src/engine.rs` (M0)

- `compile_frag` calls `lps_frontend::compile()` + `lower()` then
  `engine.compile()`. Needs renaming and signature validation.

### `lp-shader/src/frag_shader.rs` (M0)

- `LpsFragShader` needs renaming to `LpsPixelShader`
- `render_frame` needs updating: the `render` function is called with
  position args and returns color (not uniforms-only stub)

## Questions

### Q1: What exactly does `compile_pixel` validate?

**Context**: `compile_pixel` compiles GLSL via the existing path and then
validates the `render` function's signature against the output format.

Validation checks:
1. A function named `render` exists in the module
2. It takes exactly one parameter of type `vec2`
3. Its return type matches the output format's channel count:
   - `Rgba16Unorm` → `Vec4`
   - (future) `Rgb16Unorm` → `Vec3`
   - (future) `R16Unorm` → `Float`
4. Return type is float-based (not int/uint/bool)

**Suggested approach**: Hard error on any mismatch. Clear error messages
naming the expected vs actual types.

**Answer**: Yes. Hard error on mismatch. Validate: `render` exists, takes
`vec2`, returns float type matching format channel count.

### Q2: What metadata does `LpsPixelShader` carry?

**Context**: The M0 `LpsFragShader` has `meta: LpsModuleSig`,
`output_format: TextureStorageFormat`. For the pixel shader:

- `meta` still carries the full `LpsModuleSig` (all functions, uniforms,
  globals)
- `output_format` stays
- Do we need anything new? The `render` function index/offset for fast
  lookup? Or just find it by name each time?

**Suggested approach**: For M1, store the function name or index. Both
fast-path backends (Cranelift `DirectCall`, Native `NativeJitDirectCall`)
support "resolve by name once, call by handle forever" — but those are
backend-specific types, not on `LpvmInstance`. Per-pixel direct-call
handles are an M2 concern (pixel loop). For M1, name-based is sufficient.

**Answer**: Store render function index in `LpsPixelShader` for metadata.
Per-pixel direct-call optimization deferred to M2.

### Q3: Should we rename `compile_frag` → `compile_pixel` in this plan?

**Context**: M0 shipped `LpsEngine::compile_frag` and `LpsFragShader`.
This plan changes the concept from fragment to pixel shader. The M0 code
is fresh (just committed), no external consumers.

**Suggested approach**: Rename in this plan. `compile_frag` → `compile_pixel`,
`LpsFragShader` → `LpsPixelShader`. Clean break, matches the concept.

**Answer**: Yes. Rename in this plan. `compile_frag` → `compile_pixel`,
`LpsFragShader` → `LpsPixelShader`, `frag_shader.rs` → `pixel_shader.rs`.

### Q4: Does anything in `lps-frontend` need to change?

**Context**: The `render(vec2 pos) -> vec4` convention is just a regular
GLSL function. `compile()` + `lower()` already handle it. No new frontend
path needed.

However: does `extract_functions` correctly handle a function named
`render`? Does anything special happen with `main`?

With vertex stage, naga creates a synthetic entry point for our appended
`void main() {}`. The entry point calls no user functions. `render` ends
up in `module.functions` as a normal function. `extract_functions` picks
it up.

**Suggested approach**: No frontend changes. Verify with a test that
`compile("vec4 render(vec2 pos) { return vec4(0); }")` produces a
`NagaModule` with `render` in `functions` with the expected signature.

**Answer**: No frontend changes. Verify with tests during implementation.

### Q5: What about `LpsModuleSig` — any changes needed?

**Context**: The pixel shader contract says `render` has a specific
signature, but `LpsModuleSig` doesn't know about pixel shaders — it's
a general module metadata type.

Should we add pixel-shader-specific metadata to `LpsModuleSig`, or keep
it general and let `lp-shader` do the validation on top?

**Suggested approach**: Keep `LpsModuleSig` general. The pixel shader
contract is enforced by `lp-shader::compile_pixel`, not by the frontend.
`LpsModuleSig` stays as a neutral metadata type. `LpsPixelShader` stores
any pixel-shader-specific metadata (output format, render function index).

**Answer**: No changes to `LpsModuleSig`. Keep it general. Pixel-specific
metadata lives on `LpsPixelShader`.
