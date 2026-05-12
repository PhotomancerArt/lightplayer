# lp-shader Texture Access Roadmap — Notes

## Scope of the effort

Implement shader-side texture reads in `lp-shader`, following the design in
`docs/design/lp-shader-texture-access.md`.

The effort covers:

- A logical `Texture2D`/`sampler2D` type in shared shader metadata.
- A compile-time texture binding contract keyed by sampler uniform name.
- Uniform descriptor ABI lowering for texture uniforms:
  `{ ptr: u32, width: u32, height: u32, row_stride: u32 }`.
- Strict validation and diagnostics for descriptor/shader/runtime mismatches.
- `texelFetch(sampler2D, ivec2, 0)` as the foundation operation.
- `texture(sampler2D, vec2)` with nearest/linear filtering and
  clamp/repeat/mirror-repeat wrapping.
- Height-one optimization support for palettes/gradients represented as
  width-by-one 2D textures.
- Filetest support for texture specs, inline texture fixtures, and diagnostic
  cases, with a path to future wgpu comparison.
- `lp-shader` API integration so lpfx/domain can pass texture policy and
  runtime `LpsTextureBuf` values without knowing the internal ABI details.

Out of scope:

- WGSL source input.
- Real wgpu backend support.
- Mipmaps, automatic LOD, derivatives, `textureGrad`, and `textureLod`.
- 3D textures, cubemaps, texture arrays, depth/comparison samplers, gather, and
  anisotropic filtering.
- `clamp_to_border`.
- Palette stop baking and resource routing; higher layers own those concerns.

## Current state of the codebase

- `lp-shader/lp-shader/src/engine.rs` exposes `LpsEngine::compile_px`, which
  compiles GLSL, validates `render(vec2 pos)`, and synthesizes a
  format-specific `__render_texture_<format>` function for output writes.
- `lp-shader/lp-shader/src/texture_buf.rs` defines `LpsTextureBuf`, backed by
  an `LpvmBuffer` whose memory is guest-addressable. It already carries
  `width`, `height`, and `TextureStorageFormat`.
- `lp-shader/lps-shared/src/texture_format.rs` defines `TextureStorageFormat`
  with `R16Unorm`, `Rgb16Unorm`, and `Rgba16Unorm`.
- `lp-shader/lps-shared/src/types.rs` has no logical texture/sampler type yet.
- `lp-shader/lps-shared/src/lps_value_f32.rs` and
  `lp-shader/lps-shared/src/lps_value_q32.rs` do not have texture values yet.
- `lp-shader/lps-frontend/src/naga_types.rs` maps Naga types to `LpsType`, but
  sampler/image types are currently unsupported.
- `lp-shader/lps-filetests` compiles through LPVM backends directly and uses
  `// run:` plus uniform directives. It does not yet allocate/populate texture
  fixtures or bind texture uniforms.
- The existing output path proves the format-specialized write side; this
  roadmap adds the read side.

## Questions that need to be answered

All design-level questions were answered in
`docs/design/lp-shader-texture-access.md`. No additional roadmap-blocking
questions are open before overview/milestone iteration.

Resolved decisions carried into this roadmap:

- GLSL remains the v0 source language; internals are WGSL-shaped.
- `Texture2D` is a logical `LpsType`, not a user-visible fake struct.
- Texture uniforms lower to a uniform descriptor ABI.
- Compile-time texture binding specs live in shared vocabulary and are keyed by
  sampler uniform name.
- Runtime texture data comes from `LpsTextureBuf`; source routing belongs to
  lpfx/domain, not `lp-shader`.
- Validation is strict and fail-fast.
- Implementation starts foundation-first with `texelFetch`.
- `texture` sampling, wrap modes, and height-one palette helpers build on the
  foundation.
- Filetests are a first-class deliverable and should be backend-neutral enough
  for future wgpu comparison.

