# Scope Of Work

Milestone 3c makes texture reads merge-ready by validating runtime texture
bindings before execution and proving exact `texelFetch` behavior across the
current filetest backend matrix.

The important architecture decision is to separate host resource metadata from
the guest ABI token:

- The guest ABI remains `LpsTexture2DDescriptor { ptr, width, height, row_stride }`.
- The host runtime value becomes self-describing enough to validate format and
  layout before descriptor lanes are written into LPVM uniforms.
- Texture layout invariants are format-specific, so today's 16-bit formats stay
  naturally aligned without imposing a false even-stride rule on future 8-bit
  formats.

Out of scope:

- Changing `LpsTexture2DDescriptor` ABI layout.
- Adding new formats.
- Filtered `texture()` sampling, wrap semantics, mipmaps, or `lod != 0`.
- Product-level lpfx/lp-domain texture routing.
- Emulator ISA-profile gating.

# File Structure

```text
lp-shader/
├── lps-shared/
│   └── src/
│       ├── texture_format.rs          # UPDATE: host texture value/binding metadata + layout helpers
│       ├── lps_value_f32.rs           # UPDATE: Texture2D carries typed host value
│       ├── lps_value_q32.rs           # UPDATE: Q32 conversion preserves typed texture metadata
│       └── lib.rs                     # UPDATE: re-export new texture value type
├── lp-shader/
│   └── src/
│       ├── lib.rs                     # UPDATE: re-export new public texture value type
│       ├── texture_buf.rs             # UPDATE: construct typed texture values from LpsTextureBuf
│       ├── runtime_texture_validation.rs # NEW: validate runtime texture bindings vs specs
│       ├── px_shader.rs               # UPDATE: call validation before set_uniform
│       └── tests.rs                   # UPDATE: runtime validation unit/integration coverage
├── lpvm/
│   └── src/
│       ├── lpvm_abi.rs                # UPDATE: flatten typed texture value to descriptor lanes
│       ├── lpvm_data_q32.rs           # UPDATE: typed texture value storage/round-trip
│       └── set_uniform.rs             # UPDATE: tests for typed texture writes, no raw UVec4
└── lps-filetests/
    ├── src/test_run/
    │   └── texture_fixture.rs         # UPDATE: bind fixtures through typed texture values
    └── filetests/textures/
        ├── positive_minimal_fixture_design_doc.glsl # UPDATE/REPLACE: real texelFetch assertion
        └── texelfetch_*.glsl          # UPDATE: backend matrix comments if needed

docs/roadmaps/2026-04-24-lp-shader-texture-access/
└── m3c-runtime-validation-backend-filetests/
    ├── 00-notes.md
    ├── 00-design.md
    ├── 01-host-texture-value-and-layout-invariants.md
    ├── 02-public-runtime-texture-validation.md
    ├── 03-filetest-fixture-binding-and-negative-coverage.md
    ├── 04-backend-matrix-and-design-doc-fixture.md
    └── 05-cleanup-summary-and-validation.md
```

# Conceptual Architecture

```text
LpsTextureBuf
  owns/knows: buffer, guest ptr, width, height, format, byte_len, row_stride
        │
        │ to_texture2d_value()
        ▼
LpsTexture2DValue
  host value: descriptor + format + byte_len
  ABI token: descriptor lanes only
        │
        │ LpsValueF32::Texture2D(value)
        ▼
LpsPxShader::render_frame
  apply_uniforms()
    ├─ find uniform member
    ├─ if Texture2D: validate value vs meta.texture_specs[name]
    └─ set_uniform(name, value)
        │
        ▼
LPVM set_uniform / ABI flattening
  writes descriptor.ptr, width, height, row_stride as four u32 lanes
        │
        ▼
M3b texelFetch LPIR
  uses descriptor lanes for bounds, row_stride address math, Load16U, Unorm16toF
```

# Main Components

## Host Texture Value

Add a shared host value type, for example `LpsTexture2DValue`, near
`LpsTexture2DDescriptor` in `lps-shared/src/texture_format.rs`.

It should contain:

- `descriptor: LpsTexture2DDescriptor`
- `format: TextureStorageFormat`
- `byte_len: u32` or `usize` equivalent for validating footprint against the
  backing allocation

`LpsTextureBuf` should construct this value from trusted allocation metadata.
The descriptor remains accessible for ABI flattening and low-level diagnostics,
but public callers should not need to hand-write descriptor lanes for normal
binding.

## Format-Specific Layout Invariants

Introduce helpers on `TextureStorageFormat` or the new host value type:

- `bytes_per_pixel()`
- `channel_count()`
- `required_load_alignment()`

For current formats, `required_load_alignment()` is `2` because the generated
fetch path uses halfword channel loads. Future `R8`-style formats can return
`1`, and future 32-bit channel formats can return `4`.

Runtime validation should check:

- `width > 0` and `height > 0`
- `format == TextureBindingSpec::format`
- `shape=height-one` implies `height == 1`
- `ptr` is aligned to `required_load_alignment`
- `row_stride >= width * bytes_per_pixel`
- `row_stride` is aligned to `required_load_alignment`
- total footprint fits in `byte_len`, using padded-row math:
  `row_stride * (height - 1) + width * bytes_per_pixel`

Do not require tight packing. Padded rows and future subviews remain ABI-compatible.

## Runtime Validation

Add `runtime_texture_validation.rs` in the public `lp-shader` crate. It should
own validation of a single named runtime texture binding against a
`TextureBindingSpec`, returning `LpsError::Render` with actionable messages.

`LpsPxShader::apply_uniforms` should call this module before `inner.set_uniform`
for texture uniforms. This keeps validation on the same public path embedders
use for `render_frame`.

Missing texture fields remain handled by the existing uniform-struct field
lookup. Type mismatches should continue to flow through typed value matching and
LPVM uniform encoding, but tests should assert the user-facing `render_frame`
error remains clear.

## ABI Flattening

LPVM does not need format or byte-length metadata. When writing a texture
uniform, `lpvm` should flatten only the embedded descriptor:

```text
Texture2D(value) -> [value.descriptor.ptr,
                     value.descriptor.width,
                     value.descriptor.height,
                     value.descriptor.row_stride]
```

This preserves the existing guest ABI while improving host validation.

## Filetests

The filetest harness should bind texture fixtures through the typed host value,
not by manually constructing descriptor-only texture values. Existing fixture
setup validation stays useful and should remain in the harness.

The design-doc fixture should become a real behavior test by using
`texelFetch` and asserting exact output. The existing positive format tests
should continue to run on the default backend matrix (`rv32n.q32`, `rv32c.q32`,
`wasm.q32`).
