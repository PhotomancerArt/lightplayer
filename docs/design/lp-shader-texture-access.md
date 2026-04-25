# lp-shader Texture Access

## Overview

This design defines how `lp-shader` programs read from textures: the shader
surface, compile-time binding contract, runtime ABI, validation rules, feature
scope, and filetest strategy.

The near-term source language remains GLSL, but the internal model is shaped to
map cleanly to WGSL/wgpu later. In GLSL, shaders declare `sampler2D` uniforms
and call standard functions such as `texelFetch` and `texture`. Outside the
shader source, callers provide a strict descriptor map keyed by sampler uniform
name. That descriptor map gives `lp-shader` the facts needed for lowering:
storage format, filter policy, wrap policy, and shape hints.

Runtime texture data is represented by `LpsTextureBuf`. In the guest ABI, a
logical texture uniform lowers to a small uniform descriptor containing a guest
pointer and dimensions. Higher layers such as lpfx/domain decide where textures
come from; `lp-shader` only validates and consumes the texture binding contract.

## Goals

- Support texture reads needed by LightPlayer effects, transitions, and
  palette/gradient lookup.
- Keep GLSL as the v0 shader authoring surface while designing the internal
  model around WGSL/wgpu concepts.
- Avoid per-sample dynamic format/filter/wrap dispatch on the RV32/Q32 path.
- Make texture binding strict and diagnosable: missing specs, mismatched
  formats, and broken shape promises should fail early.
- Treat filetests as the primary validation tool, with fixtures that can later
  be reused for wgpu comparison.
- Support efficient height-one texture lookup without adding a separate 1D
  resource type.

## Non-Goals

- WGSL source input. WGSL parsing/source support should be a later plan, likely
  tied to real wgpu backend work.
- Mipmaps, automatic LOD, and derivative-based sampling.
- 3D textures, cubemaps, texture arrays, depth/comparison samplers, gather, and
  anisotropic filtering.
- `clamp_to_border`. It is useful for some warp/zoom effects, but v0 avoids
  border-color sampler state until a concrete effect requires it.
- Palette stop baking. `lp-shader` defines texture sampling primitives; higher
  layers bake gradient/palette stops into textures.
- Resource routing. lpfx/domain decide whether a texture comes from an upstream
  visual, bus, generated palette, or some future artifact source.

## Background

`lp-shader` already has the write side of CPU texture rendering:

- `LpsEngine::compile_px` compiles GLSL through `lps-frontend`, lowers to LPIR,
  validates `render(vec2 pos)`, and synthesizes a format-specific
  `__render_texture_<format>` function.
- `LpsTextureBuf` wraps an `LpvmBuffer`; that memory is guest-addressable and
  already suitable for shader reads.
- `TextureStorageFormat` currently includes `R16Unorm`, `Rgb16Unorm`, and
  `Rgba16Unorm`.

The missing feature is shader-side texture reads. Product-facing use cases are:

- Effects that read one input texture and write an output texture.
- Transitions that read two input textures and blend or warp them.
- Palettes/gradients, likely baked by a higher layer into height-one textures.

The `lp-domain` work already stores shaders in TOML visual definitions and uses
conventions such as:

```glsl
uniform sampler2D inputColor;
uniform sampler2D inputA;
uniform sampler2D inputB;
```

Those names and routes belong to lpfx/domain. `lp-shader` needs only a strict
binding contract for sampler uniforms that appear in the shader.

## Design

### Source Surface

GLSL remains the v0 source language. Shaders declare texture inputs as
`sampler2D` uniforms and use standard read functions:

```glsl
uniform sampler2D inputColor;

vec4 read_px(ivec2 p) {
    return texelFetch(inputColor, p, 0);
}

vec4 read_uv(vec2 uv) {
    return texture(inputColor, uv);
}
```

Filter and wrap policy are not encoded in GLSL layout qualifiers or in a large
family of LP-specific function names. They are supplied by binding metadata
outside the shader source. This matches the eventual WGSL/wgpu model, where
texture resource and sampler policy are separate binding concepts.

### Compile-Time Binding Contract

Compilation receives a descriptor map keyed by sampler uniform name. The exact
Rust shape can evolve, but the stable concept is:

```rust
pub struct TextureBindingSpec {
    pub format: TextureStorageFormat,
    pub filter: TextureFilter,
    pub wrap_x: TextureWrap,
    pub wrap_y: TextureWrap,
    pub shape_hint: TextureShapeHint,
}

pub enum TextureFilter {
    Nearest,
    Linear,
}

pub enum TextureWrap {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

pub enum TextureShapeHint {
    General2D,
    HeightOne,
}
```

Core descriptor vocabulary belongs in `lps-shared`, next to
`TextureStorageFormat`, so `lps-frontend`, `lp-shader`, filetests, and future
WGSL/wgpu support share one vocabulary.

Compilation should receive texture specs through a named compile descriptor
rather than growing positional arguments:

```rust
pub struct CompilePxDesc<'a> {
    pub glsl: &'a str,
    pub output_format: TextureStorageFormat,
    pub textures: BTreeMap<String, TextureBindingSpec>,
    pub compiler_config: CompilerConfig,
}
```

The compile descriptor name and exact ownership can change, but the API should
make texture binding metadata explicit.

### Logical Type and ABI

`sampler2D` maps to a logical `LpsType::Texture2D`, not to a user-visible fake
struct. Metadata and diagnostics should talk about texture/sampler uniforms,
not fields like `ptr` and `width`.

The guest ABI lowers a texture uniform to a fixed descriptor in the uniforms
region:

```rust
#[repr(C)]
pub struct Texture2DUniform {
    pub ptr: u32,
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
}
```

`row_stride` is included in v0 even when textures are tightly packed. It costs
one word and avoids a future ABI break for subviews or non-tight rows.

The public runtime API should provide typed helpers or values built from
`LpsTextureBuf`; callers should not normally hand-author raw pointer structs.
For example, a future API might expose `LpsValueF32::Texture2D(...)` or a
uniform builder helper that writes a `Texture2DUniform` from an `LpsTextureBuf`.

### Runtime Binding and Validation

At runtime, lpfx or another caller provides `LpsTextureBuf` values for the
texture uniforms. `lp-shader` validates them against the compile-time spec
before rendering.

Validation is fail-fast:

- A shader declares a `sampler2D` but no matching `TextureBindingSpec` exists:
  compile/validation error.
- A spec names a sampler that does not exist in the shader: compile/validation
  error.
- A runtime texture binding is missing: render error.
- A runtime texture has a different format from the compile-time spec: render
  error.
- `TextureShapeHint::HeightOne` is promised but runtime `height != 1`: render
  error.
- Unsupported filter/wrap combinations for a target/profile: compile error
  tied to the sampler name.

No sentinel value is required in `LpsTextureBuf` for v0. The normal safety
boundary is typed construction plus validation at binding/render time.

### Sampling Semantics

The foundation operation is:

```glsl
texelFetch(sampler2D tex, ivec2 coord, int lod)
```

Only `lod == 0` is supported in v0. `texelFetch` uses integer pixel
coordinates and the sampler's format to emit format-specific loads and
conversion. This is the first implementation slice because it proves the
descriptor contract, runtime binding, filetest support, and backend lowering.

Filtered sampling is:

```glsl
texture(sampler2D tex, vec2 uv)
```

`texture` uses normalized coordinates, the binding's filter policy, and the
binding's wrap policies. Required filter modes are `Nearest` and `Linear`.
Required wrap modes are `ClampToEdge`, `Repeat`, and `MirrorRepeat`.

`Rgb16Unorm` remains supported on the CPU path because it already exists in
`TextureStorageFormat`, but it is not WebGPU-portable. WebGPU has no 3-channel
texture formats.

### Height-One Textures

V0 texture resources are 2D only. Linear visuals such as palettes and gradients
are represented as width-by-one 2D textures. A binding may declare
`TextureShapeHint::HeightOne` to promise that runtime textures have `height == 1`.

This is an optimization hint, not a separate resource type. It lets the compiler
choose cheaper palette/gradient sampling paths while preserving one texture ABI
and one runtime buffer type.

Palette stop baking is out of scope for `lp-shader`. Higher layers should bake
UI-style gradient stops into a texture and pass that texture to `lp-shader`.

### WGSL/wgpu Mapping

The internal model intentionally resembles WGSL/wgpu:

- Logical texture resource: `Texture2D`.
- Separate sampler policy: filter and wrap in `TextureBindingSpec`.
- Format known by binding metadata rather than dynamically dispatched per
  sample.

Future WGSL support may map:

- `texelFetch` to `textureLoad`.
- `texture` with linear/nearest policy to `textureSample` with the appropriate
  sampler.
- CPU uniform descriptors to actual wgpu texture/sampler bindings.

WGSL source input is not part of this design. That work should happen when the
wgpu backend is real enough to validate the source and runtime mapping.

### Filetests

Texture filetests should extend the existing `.glsl` comment-directive style.
They need to declare both compile-time texture specs and runtime fixture data.

Example:

```glsl
// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: inputColor 3x1 rgba16unorm
//   1.0,0,0,1.0 0,1.0,0,1.0 0,0,1.0,1.0
//
// run: sample_red() ~= vec4(1.0, 0.0, 0.0, 1.0)

uniform sampler2D inputColor;

vec4 sample_red() {
    return texelFetch(inputColor, ivec2(0, 0), 0);
}
```

Fixture data uses pixel-grouped channel values, not raw bytes:

- Pixels are separated by whitespace.
- Channels inside a pixel are comma-separated with no spaces.
- Prefer normalized float channels for readable tests:
  `1.0,0,0,1.0`.
- Allow exact hex storage values for precision/boundary cases:
  `ffff,0000,0000,ffff`.
- Channel count must match the storage format.
- The parser encodes fixture values into the target texture storage format.

Tiny inline fixtures should cover v0. Sidecar image/fixture files can be added
later if larger test inputs become necessary.

Filetests should include both positive behavior tests and negative diagnostics:

- missing texture spec for a shader sampler,
- extra spec for a nonexistent sampler,
- missing runtime texture data,
- format mismatch,
- height-one promise violated,
- unsupported filter/wrap combinations,
- exact `texelFetch` results,
- approximate filtered sampling results where GPU and Q32 may differ by a small
  tolerance.

The fixture format should stay backend-neutral so future wgpu comparison can
reuse the same declarations.

## Decisions

#### GLSL v0, WGSL-shaped internals

- **Decision:** Keep GLSL as the v0 source surface, but model textures as
  WGSL-style resources plus sampler policy.
- **Why:** GLSL is more familiar and matches existing tests/examples; WGSL maps
  texture/resource concepts more directly and should shape the future.
- **Rejected alternatives:** Switch wholesale to WGSL now; encode policy in
  custom GLSL layout qualifiers; encode policy in many LP-specific builtin
  names.

#### Texture policy supplied outside shader source

- **Decision:** `TextureBindingSpec` supplies format/filter/wrap/shape by
  sampler uniform name.
- **Why:** lpfx/domain already own visual context and resource routing. Shader
  source should not need to duplicate sampler state.
- **Rejected alternatives:** Hard-code conventions only; require all texture
  policy in GLSL.

#### Texture2D is logical, descriptor is ABI

- **Decision:** Add a logical texture type/value; lower it to a fixed uniform
  descriptor in the guest ABI.
- **Why:** Keeps diagnostics and future WGSL mapping clean while preserving a
  simple CPU ABI.
- **Rejected alternatives:** Treat samplers as ordinary user structs from the
  start.

#### Strict validation

- **Decision:** Treat descriptor promises and runtime bindings as strict.
- **Why:** The system is young; fail-fast diagnostics are better than permissive
  fallbacks that hide broken contracts.
- **Rejected alternatives:** Ignore extra specs; silently fall back when shape
  hints or formats mismatch.

#### Foundation-first implementation

- **Decision:** Implement descriptor validation and `texelFetch` before
  filtered sampling and palette helpers.
- **Why:** `texelFetch` proves the full binding/ABI/filetest path with the
  smallest sampling operation.
- **Rejected alternatives:** Start with palette helper because it is
  product-visible.

## Open Questions

None for this design. Roadmap planning should still choose milestone boundaries
and exact Rust names.

## Related Documents

- `docs/design/q32.md`
- `docs/design/lpir/08-glsl-mapping.md`
- `docs/roadmaps/2026-04-16-lp-shader-textures/`
- `docs/roadmaps/2026-04-22-lp-shader-aggregates/`
- `/Users/yona/dev/photomancer/feature/lightplayer-emu-perf/docs/roadmaps/2026-04-23-lp-render-mvp/`
- `/Users/yona/dev/photomancer/feature/lightplayer-emu-perf/lp-domain/lp-domain/`

## Changelog

- 2026-04-24: Initial design captured from the texture access design-small
  discussion.
