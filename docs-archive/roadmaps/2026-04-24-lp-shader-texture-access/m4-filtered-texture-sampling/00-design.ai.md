# Scope of Work

Milestone 4 implements GLSL `texture(sampler2D, vec2)` sampling for logical
`Texture2D` uniforms using compile-time `TextureBindingSpec` metadata.

The implementation is builtin-first: frontend lowering selects a texture
sampler builtin rather than expanding all sampling math directly into LPIR. The
selected builtin is specialized by storage format and by dimensionality/shape
(`General2D` vs `HeightOne`). Filter and wrap policy remain runtime arguments
inside that format/shape-specific builtin so the plan avoids a combinatoric
symbol matrix.

In scope:

- Lower supported `texture(sampler2D, vec2)` calls.
- Implement normalized-coordinate texel-center sampling:
  `coord = uv * extent - 0.5`.
- Implement nearest and linear filtering.
- Implement clamp-to-edge, repeat, and mirror-repeat wrap helpers.
- Specialize sampler builtins by storage format and shape.
- Use a 1D specialized path for `TextureShapeHint::HeightOne`.
- Add a Rust reference sampler for tests and expected-value generation.
- Add filetests for nearest, linear, wrap modes, mixed-axis wrap, and height-one
  Y-insensitivity.

Out of scope:

- Mipmaps, implicit derivatives, `textureLod`, `textureGrad`, and nonzero LOD.
- `clamp_to_border`.
- New texture storage formats.
- Product-level texture routing and public palette helper APIs.
- wgpu execution parity.

# File Structure

```text
lp-shader/
├── lps-shared/
│   └── src/
│       └── texture_format.rs                    # UPDATE: small numeric ABI helpers for filter/wrap if needed
├── lps-builtins/
│   └── src/
│       └── builtins/
│           └── texture/                         # NEW: sampler builtins and shared sampler math
│               ├── mod.rs                       # NEW: module wiring
│               ├── sample_ref.rs                # NEW: pure reference/helper sampler math
│               ├── rgba16_unorm_q32.rs          # NEW: 2D + 1D RGBA16 sampler externs
│               ├── r16_unorm_q32.rs             # NEW: 2D + 1D R16 sampler externs
│               └── rgb16_unorm_q32.rs           # NEW/OPTIONAL: 2D + 1D RGB16 if cheap
├── lps-builtins-gen-app/
│   └── src/
│       ├── main.rs                              # UPDATE: include texture builtin namespace in generated IDs/mappings
│       └── native_dispatch_codegen.rs           # UPDATE: support texture builtin result/memory dispatch as needed
├── lps-builtin-ids/
│   └── src/                                     # GENERATED: texture builtin IDs/mappings
├── lps-frontend/
│   └── src/
│       ├── lower.rs                             # UPDATE: register texture sampler imports
│       ├── lower_expr.rs                        # UPDATE: route sampled-image texture() expressions
│       └── lower_texture.rs                     # UPDATE: select sampler builtin by spec format + shape
├── lpvm-wasm/
│   └── src/
│       └── emit/                                # UPDATE: generated/import handling if texture builtins need new shape
└── lps-filetests/
    ├── src/test_run/                            # UPDATE: expected-value helpers if kept test-side
    └── filetests/textures/
        ├── texture_nearest_rgba16_clamp.glsl    # NEW
        ├── texture_nearest_rgba16_repeat.glsl   # NEW
        ├── texture_linear_rgba16_clamp.glsl     # NEW
        ├── texture_linear_rgba16_repeat.glsl    # NEW
        ├── texture_nearest_r16.glsl             # NEW
        ├── texture_mixed_axis_wrap.glsl         # NEW
        └── texture_height_one_1d.glsl           # NEW: uv.y has no effect
```

# Conceptual Architecture

```text
GLSL texture(sampler2D, vec2)
        │
        ▼
Naga sampled-image expression
        │
        ▼
lps-frontend::lower_texture
  ├─ resolve direct Texture2D uniform
  ├─ validate matching TextureBindingSpec
  ├─ load/pass descriptor lanes
  ├─ inspect spec.format and spec.shape_hint
  ├─ if General2D:
  │    select texture2d_<format> builtin
  │    pass uv.x, uv.y, filter, wrap_x, wrap_y
  └─ if HeightOne:
       select texture1d_<format> builtin
       pass uv.x, filter, wrap_x
       intentionally drop uv.y and wrap_y
        │
        ▼
lps-builtins::builtins::texture
  ├─ format-specialized unorm loads and vec4 fill
  ├─ shape-specialized address math
  ├─ runtime filter branch:
  │    nearest helper
  │    linear helper
  ├─ runtime wrap helper(s)
  └─ write vec4 result through result pointer ABI
        │
        ▼
filetests + Rust reference sampler
  ├─ exact-ish nearest expectations
  ├─ tolerance-based linear expectations
  └─ explicit HeightOne tests proving Y is ignored
```

# Main Components

## Frontend Lowering

`lps-frontend/src/lower_texture.rs` remains the texture-specific lowering
module. M4 extends it from `texelFetch` lowering to also own
`texture(sampler2D, vec2)` lowering.

Lowering should:

- Recognize Naga's sampled-image expression for GLSL `texture`.
- Resolve the texture operand to a direct `Texture2D` uniform, following the
  strict M3a/M3b contract.
- Lookup `TextureBindingSpec` by sampler uniform name.
- Reject unsupported sampled-image forms such as explicit LOD, gradients,
  array/layered textures, or non-`Texture2D` operands.
- Select a builtin by `TextureStorageFormat` and `TextureShapeHint`.
- Pass filter and wrap policy as small integer arguments instead of selecting a
  different builtin for every policy combination.

For `TextureShapeHint::HeightOne`, the frontend intentionally lowers to a 1D
builtin and drops Y:

```text
texture(sampler2D, vec2(u, v)) + shape=height-one
  -> texture1d_<format>(out, desc, u, filter, wrap_x)
```

This keeps shader source aligned with practical GLSL/GPU usage while allowing
the CPU path to optimize common palette/gradient lookups. Runtime validation
from M3c must continue to reject bindings where `shape=height-one` but the
runtime texture has `height != 1`.

## Texture Builtins

Add texture sampling builtins under `lps-builtins/src/builtins/texture/`.

Initial extern shape should prefer one builtin per format + shape, for example:

```rust
pub extern "C" fn __lp_texture2d_rgba16_unorm_q32(
    out: *mut i32,
    ptr: u32,
    width: u32,
    height: u32,
    row_stride: u32,
    u: i32,
    v: i32,
    filter: u32,
    wrap_x: u32,
    wrap_y: u32,
);

pub extern "C" fn __lp_texture1d_rgba16_unorm_q32(
    out: *mut i32,
    ptr: u32,
    width: u32,
    row_stride: u32,
    u: i32,
    filter: u32,
    wrap_x: u32,
);
```

Exact signatures can be adjusted to match existing builtin-generator and ABI
patterns, but keep these properties:

- Result is a vec4/Q32 value written through a result pointer.
- Texture storage pointer and descriptor lanes come from the existing
  `LpsTexture2DDescriptor` ABI.
- Format-specific modules own channel load offsets and vec4 fill.
- Filter dispatch calls small internal helpers for nearest and linear.
- Wrap dispatch is runtime, shared, and explicit.

## Reference Sampler

Add a pure Rust reference sampler for tests. It should encode:

- texel-center coordinate convention: `coord = uv * extent - 0.5`;
- nearest selection;
- linear interpolation;
- clamp-to-edge, repeat, and mirror-repeat wrap;
- 2D sampling;
- 1D height-one sampling that ignores Y.

The reference implementation may live initially in `lps-builtins` if it can be
shared with builtin unit tests, or in filetest test utilities if keeping it out
of production code is cleaner. Prefer `no_std`-friendly pure helpers so moving
between those homes remains cheap.

## Builtin ABI and Dispatch

Texture builtins should use the existing generated builtin machinery rather
than hand-editing generated files. After adding externs, regenerate builtin IDs,
module lists, Cranelift ABI tables, WASM import types, and runtime dispatch.

Existing builtin infrastructure already supports the needed concepts:

- builtin IDs and signatures generated from `extern "C"` functions;
- result-pointer-style vector returns for LPFN builtins;
- native builtins treating VMContext/guest pointers as address words;
- Wasmtime builtin dispatch receiving shared `env.memory`.

If texture builtins expose a new module namespace or result-pointer pattern,
generalize the current LPFN-specific result-pointer handling rather than
duplicating fragile special cases.

## Filetests

Add positive filetests for:

- nearest clamp sampling;
- nearest repeat sampling;
- linear clamp sampling with tolerance;
- linear repeat sampling with tolerance;
- mixed X/Y wrap policy on 2D;
- height-one/1D sampling where varying `uv.y` does not change the result;
- at least one R16 path to prove non-RGBA vec4 fill.

Add negative diagnostics for unsupported cases introduced by M4, especially
unsupported texture operand shapes or any format/shape combination deliberately
deferred from v0.

