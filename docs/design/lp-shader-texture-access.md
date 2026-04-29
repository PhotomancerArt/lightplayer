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
pointer and dimensions. Callers outside `lp-shader` decide how textures are
produced and routed; `lp-shader` validates and consumes the compile-time binding
contract and runtime buffers supplied to it.

## Goals

- Support texture reads needed by LightPlayer effects, transitions, and
  palette/gradient lookup.
- Keep GLSL as the v0 shader authoring surface while designing the internal
  model around WGSL/wgpu concepts.
- Avoid per-sample dynamic format/filter/wrap dispatch on the RV32/Q32 path.
- Make texture binding strict and diagnosable: missing specs, mismatched
  formats, and broken shape promises should fail early.
- Treat GLSL filetests as the primary integration signal for texture reads; a
  dedicated wgpu comparison harness is future work.
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

Downstream tooling may use conventions such as:

```glsl
uniform sampler2D inputColor;
uniform sampler2D inputA;
uniform sampler2D inputB;
```

Uniform naming and routing are not enforced inside `lp-shader`; it needs only a
strict `TextureBindingSpec` entry per `sampler2D` uniform leaf that appears in
GLSL—including nested fields inside uniform structs (see below).

## Design

### Source Surface

GLSL remains the v0 source language. Shaders declare texture inputs as
`sampler2D` uniforms or as `sampler2D` fields inside uniform structs, and use
standard read functions:

```glsl
uniform sampler2D inputColor;

vec4 read_px(ivec2 p) {
    return texelFetch(inputColor, p, 0);
}

vec4 read_uv(vec2 uv) {
    return texture(inputColor, uv);
}
```

Uniform structs may carry scalars alongside textures in one bundle:

```glsl
struct Params {
    float amount;
    sampler2D gradient;
};

uniform Params params;

vec4 sample_gradient(vec2 uv) {
    return texture(params.gradient, uv) * params.amount;
}
```

For nested fields, **`TextureBindingSpec` keys use the same canonical dotted path
/string** as runtime uniform paths and tooling: root field name joined with `.`,
e.g. `params.gradient`. Top-level samplers remain single identifiers, e.g.
`inputColor`. Arrays of textures or indexed paths such as
`params.gradients[0]` are not supported in this contract.

Filter and wrap policy are not encoded in GLSL layout qualifiers or in a large
family of LP-specific function names. They are supplied by binding metadata
outside the shader source. This matches the eventual WGSL/wgpu model, where
texture resource and sampler policy are separate binding concepts.

### Compile-Time Binding Contract

Compilation receives a map keyed by **canonical texture path** (`String` →
`TextureBindingSpec`): either a top-level uniform name (`inputColor`) or a dotted
path for a nested `sampler2D` field (`params.gradient`). The key must match
the uniform leaf’s path in the module layout. Shared vocabulary lives in
`lps-shared` next to `TextureStorageFormat`.

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

Pixel shaders compile through `CompilePxDesc`, which holds
`textures: BTreeMap<String, TextureBindingSpec>`. Prefer building specs with
`CompilePxDesc::new` and `CompilePxDesc::with_texture_spec` rather than wiring ad hoc maps.

Convenience constructors live in `lp_shader::texture_binding`:

- `texture_binding::texture2d(format, filter, wrap_x, wrap_y)` — general 2D sampling.
- `texture_binding::height_one(format, filter, wrap_x)` — height-one strip; sets
  `wrap_y` to `ClampToEdge` and pairs with `TextureShapeHint::HeightOne`.

Example:

```rust
use lp_shader::{CompilePxDesc, texture_binding};
use lps_shared::{TextureFilter, TextureStorageFormat, TextureWrap};

let desc = CompilePxDesc::new(glsl, TextureStorageFormat::Rgba16Unorm, compiler_config)
    .with_texture_spec(
        "inputColor",
        texture_binding::texture2d(
            TextureStorageFormat::Rgba16Unorm,
            TextureFilter::Nearest,
            TextureWrap::ClampToEdge,
            TextureWrap::ClampToEdge,
        ),
    )
    .with_texture_spec(
        "params.gradient",
        texture_binding::height_one(
            TextureStorageFormat::Rgba16Unorm,
            TextureFilter::Nearest,
            TextureWrap::ClampToEdge,
        ),
    );
```

### Logical Type and ABI

`sampler2D` maps to a logical `LpsType::Texture2D`, not to a user-visible fake
struct. Metadata and diagnostics should talk about texture/sampler uniforms,
not fields like `ptr` and `width`.

The guest ABI lowers a texture uniform to a fixed descriptor in the uniforms
region:

```rust
#[repr(C)]
pub struct LpsTexture2DDescriptor {
    pub ptr: u32,
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
}
```

`row_stride` is included in v0 even when textures are tightly packed. It costs
one word and avoids a future ABI break for subviews or non-tight rows.

Host-side runtime values use `LpsTexture2DValue`: the guest descriptor plus
`TextureStorageFormat` and `byte_len` (backing allocation size). Only the four
`LpsTexture2DDescriptor` lanes participate in guest uniform memory; format and
byte length are used by host validation, not written into LPVM uniform slots.

Callers with an allocated `LpsTextureBuf` should prefer `to_texture2d_value()`
(or `to_named_texture_uniform(name)` for `(String, LpsValueF32)` fields) so
validation sees consistent format and storage size. `to_texture2d_descriptor()`
remains available for the raw guest layout alone.

### Runtime Binding and Validation

At runtime, the embedder provides buffer-backed texture values for the sampler
uniforms (typically `LpsTextureBuf` → `LpsTexture2DValue`). `lp-shader`
validates them against the compile-time spec before rendering.

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

Filtered sampling with an unsupported storage format is rejected during
frontend lowering with a diagnostic that includes the sampler uniform name (for
example `Rgb16Unorm` with `texture()` — see below).

No sentinel value is required in `LpsTextureBuf` for v0. The normal safety
boundary is typed construction plus validation at binding/render time.

### Sampling Semantics

The foundation operation is:

```glsl
texelFetch(sampler2D tex, ivec2 coord, int lod)
```

Only `lod == 0` is supported in v0. `texelFetch` uses integer pixel coordinates
and the sampler's format to emit format-specific loads and conversion.

Supported storage formats for `texelFetch`:

- `R16Unorm`
- `Rgb16Unorm`
- `Rgba16Unorm`

Filtered sampling is:

```glsl
texture(sampler2D tex, vec2 uv)
```

`texture` uses normalized coordinates, the binding's filter policy, and the
binding's wrap policies.

Supported storage formats for filtered `texture()` today:

- `R16Unorm`
- `Rgba16Unorm`

`Rgb16Unorm` is supported for `texelFetch` but **not** for filtered `texture()`
in the current lowering: attempting filtered sampling on `Rgb16Unorm` is a
compile-time error tied to the sampler. Adding a dedicated format path or builtin
would be prerequisite if filtered RGB16 becomes required.

Required filter modes are `Nearest` and `Linear`. Required wrap modes are
`ClampToEdge`, `Repeat`, and `MirrorRepeat`.

### Height-One Textures

V0 texture resources are 2D only. Linear visuals such as palettes and gradients
are represented as width-by-one 2D textures. A binding may declare
`TextureShapeHint::HeightOne` to promise that runtime textures have `height == 1`.

This is an optimization hint, not a separate resource type. GLSL still uses
`sampler2D` and `vec2` UVs; lowering selects a path that **ignores `uv.y`** and
does not apply vertical wrap semantics for that sampler (the
`texture_binding::height_one` helper fixes `wrap_y` to `ClampToEdge` on the unused
axis). Validation rejects runtime textures with `height != 1` when this hint is set.

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

Canonical texture GLSL tests live under
`lp-shader/lps-filetests/filetests/texture/`. They extend the usual `.glsl`
comment-directive style: each sampler **leaf path** needs a matching compile-time
`// texture-spec:` and runtime `// texture-data:` block before shader source.
Use the dotted name for nested uniforms (same string as compile-time specs),
e.g. `params.gradient`.

Run the texture corpus with the repo script (not Rust unit tests alone), for example:

```bash
scripts/filetests.sh --target wasm.q32,rv32n.q32,rv32c.q32 texture/
```

Example:

```glsl
// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: inputColor 3x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0 0.0,0.0,1.0,1.0

uniform sampler2D inputColor;

vec4 sample_red() {
    return texelFetch(inputColor, ivec2(0, 0), 0);
}

// run: sample_red() ~= vec4(1.0, 0.0, 0.0, 1.0)
```

Nested sampler in a uniform struct:

```glsl
// texture-spec: params.gradient format=rgba16unorm filter=nearest wrap=clamp shape=height-one
// texture-data: params.gradient 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0

struct Params {
    float amount;
    sampler2D gradient;
};

uniform Params params;

vec4 palette_sample() {
    return texture(params.gradient, vec2(0.75, 0.0)) * params.amount;
}
```

Integration tests under `filetests/texture/` add `// run:` expectations against the fixture.

Texture directives (see `lp-shader/lps-filetests/README.md`):

- `// texture-spec: <path> format=<...> filter=<...> shape=<...>` plus either
  `wrap=<both axes>` or both `wrap_x=` and `wrap_y=`.
  `<path>` is a single token: a simple name (`inputColor`) or a dotted path
  (`params.gradient`). Indexed segments (`foo[0]`) are not supported in
  directives.
  - Formats: `r16unorm`, `rgb16unorm`, `rgba16unorm`.
  - Filters: `nearest`, `linear`.
  - Wraps: `clamp` (or `clamp-to-edge`), `repeat`, `mirror-repeat` (underscore
    variants accepted).
  - Shapes: `2d` (`General2D`), `height-one` or `height_one` (`HeightOne`).
- `// texture-data: <path> <W>x<H> <format>` followed by comment lines listing
  pixels in row-major order.

Fixture data uses pixel-grouped channel values, not raw bytes:

- Pixels are separated by whitespace.
- Channels inside a pixel are comma-separated with no spaces.
- Prefer normalized float channels for readable tests (`1.0,0.0,0.0,1.0`).
- Allow exact four-digit hex channel values per channel where needed.
- Channel count must match the storage format.
- The parser encodes fixture values into the target texture storage format.

Tiny inline fixtures cover the shipped milestones. Sidecar fixtures remain a
possible extension if larger inputs are needed.

Negative tests in the same directory cover missing/extra specs, fixture/shape
mismatches, unsupported filtered formats, and parse failures.

A future wgpu comparison harness may reuse the same declarations; that tooling
is not part of the shipped validation story yet.

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
- **Why:** Resource routing lives outside `lp-shader`. Shader source should not
  duplicate sampler state.
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

## Future work (explicitly not shipped here)

- Dedicated wgpu comparison runner for the same filetest fixtures.
- WGSL source input (paired with a real wgpu backend).
- `clamp_to_border` sampler addressing.
- Mipmaps and manual/explicit LOD beyond implicit base level for `texture()`.
- Larger sidecar fixtures if inline pixel blocks become unwieldy.

## Related Documents

- `docs/design/q32.md`
- `docs/design/lpir/08-glsl-mapping.md`
- `docs/roadmaps/2026-04-16-lp-shader-textures/`
- `docs/roadmaps/2026-04-22-lp-shader-aggregates/`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/`

## Changelog

- 2026-04-28 (afternoon): Document nested `sampler2D` in uniform structs; dotted
  `TextureBindingSpec` / filetest keys (`params.gradient`); disallow indexed
  texture directive names.
- 2026-04-28: Document M1–M5 shipped behavior (APIs, formats, filetests, validation
  split between guest descriptor and host `LpsTexture2DValue` metadata).
- 2026-04-24: Initial design captured from the texture access design-small
  discussion.
