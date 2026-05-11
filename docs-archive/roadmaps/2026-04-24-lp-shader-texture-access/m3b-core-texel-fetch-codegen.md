# Milestone 3b: Core `texelFetch` codegen

## Goal

Implement the core `texelFetch(sampler2D, ivec2, 0)` LPIR lowering path using
the texture-aware contract from Milestone 3a.

This milestone is the data-path slice: descriptor field use, coordinate policy,
offset math, unorm16 storage loads, conversion to shader float/Q32 values, and
GLSL-compatible `vec4` channel fill.

## Suggested plan location

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m3b-core-texel-fetch-codegen/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

### In scope

- Lower supported Naga/GLSL `texelFetch(sampler2D, ivec2, 0)` calls into LPIR.
- Load descriptor lanes from the `Texture2D` uniform ABI:
  `ptr`, `width`, `height`, `row_stride`.
- Implement the v0 out-of-range coordinate policy chosen during planning.
- Compute texel byte addresses using integer coordinates and descriptor
  `row_stride`, not an assumed tight row stride.
- Emit format-specialized channel loads and conversion for:
  - `R16Unorm`,
  - `Rgb16Unorm`,
  - `Rgba16Unorm`.
- Convert unorm16 storage channels with existing `Unorm16toF` behavior so Q32
  and native float paths stay consistent with the rest of LPIR.
- Return GLSL-compatible `vec4` sample values:
  - R formats fill missing G/B with `0.0` and A with `1.0`,
  - RGB formats return alpha `1.0`,
  - RGBA formats return all four loaded channels.
- Add focused tests/filetests that exercise generated behavior on at least one
  backend during this milestone.

### Out of scope

- Runtime validation of host-provided `LpsTextureBuf` values.
- Broad backend matrix validation across all LPVM targets.
- Public API helpers for texture binding.
- Normalized-coordinate `texture()` sampling, filtering, or wrap modes.
- Mipmaps or any `lod != 0` behavior.

## Key decisions

- `texelFetch` remains an inlined LPIR sequence for v0; no dedicated texture
  opcode or runtime format switch is introduced.
- `Rgb16Unorm` is supported on CPU because it already exists in
  `TextureStorageFormat`, even though it is not WebGPU-portable.
- Address math should honor `row_stride` from the descriptor so future non-tight
  rows or subviews do not require an ABI break.

## Deliverables

- Frontend lowering from supported `texelFetch` expressions to LPIR load and
  conversion ops.
- Shared helper(s), if useful, for descriptor lane handling, coordinate bounds,
  format channel layout, and vec4 fill.
- Positive exact-value tests for `R16Unorm`, `Rgb16Unorm`, and `Rgba16Unorm`
  on the initial validation target.
- Negative tests for the coordinate/LOD policy if not already covered by M3a.

## Dependencies

- Depends on Milestone 3a for texture-aware lowering metadata, operand
  resolution, and LOD diagnostics.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: the implementation is compact but correctness depends on
coordinate policy, Q32 conversion details, row-stride math, and format-specific
channel fill. A full plan keeps those decisions explicit.

**Suggested chat opener:**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

