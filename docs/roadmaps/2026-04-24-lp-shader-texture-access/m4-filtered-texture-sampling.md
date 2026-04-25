# Milestone 4: Filtered `texture()` sampling and wrap modes

## Goal

Implement normalized-coordinate `texture(sampler2D, vec2)` sampling with
nearest/linear filtering and clamp/repeat/mirror-repeat wrap modes, using the
compile-time sampler policy from `TextureBindingSpec`.

## Suggested plan location

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m4-filtered-texture-sampling/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

### In scope

- Lower GLSL `texture(sampler2D, vec2)` for logical `Texture2D` uniforms.
- Implement normalized-coordinate addressing from `uv` to texture coordinates.
- Implement `TextureFilter::Nearest`.
- Implement `TextureFilter::Linear` for 2D bilinear sampling.
- Implement wrap modes from `TextureBindingSpec`:
  - `ClampToEdge`,
  - `Repeat`,
  - `MirrorRepeat`.
- Respect per-axis wrap policy (`wrap_x`, `wrap_y`).
- Reuse the format-specialized `texelFetch` load/conversion machinery from
  Milestone 3.
- Add filetests for:
  - nearest sampling,
  - linear interpolation,
  - clamp,
  - repeat,
  - mirror-repeat,
  - exact vs approximate expectations where needed.
- Add diagnostics for unsupported combinations or malformed sampler policy.

### Out of scope

- Mipmaps, LOD, derivatives, `textureGrad`, `textureLod`.
- `clamp_to_border`.
- Anisotropic filtering and gather.
- Height-one palette helper as a public convenience API (Milestone 5).
- wgpu execution parity runner.

## Key decisions

- Filter and wrap policy come from `TextureBindingSpec`, not shader source.
- Linear filtering is supported because effects/transitions need warped,
  zoomed, and twisted inputs, but CPU performance constraints remain visible
  through the `Nearest` policy.
- `MirrorRepeat` is part of this roadmap, not an indefinite future note.
- Filtering tests may use tolerances; direct `texelFetch` tests remain exact.

## Deliverables

- Frontend lowering for `texture(sampler2D, vec2)`.
- Shared sampling helpers or lowering patterns for nearest, bilinear, clamp,
  repeat, and mirror-repeat.
- Filetests covering filter/wrap semantics on supported LPVM backends.
- Performance notes or diagnostics documenting expensive modes on RV32.

## Dependencies

- Depends on Milestone 3 for texture descriptor loading, format-specialized
  fetches, and runtime validation.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: This milestone introduces coordinate normalization, per-axis wrap
logic, bilinear interpolation, and tolerance-based testing. It needs a full plan
to avoid scattering subtly different sampling math across call sites.

**Suggested chat opener:**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

