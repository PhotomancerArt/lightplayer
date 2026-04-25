# Milestone 3: `texelFetch` lowering and backend validation

## Goal

Implement `texelFetch(sampler2D, ivec2, 0)` as the foundation texture-read
operation, with format-specialized loads, strict validation, and filetest
coverage across LPVM backends.

## Suggested plan location

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m3-texel-fetch/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

### In scope

- Lower Naga/GLSL `texelFetch` calls on logical `Texture2D` uniforms.
- Support `lod == 0`; reject nonzero or dynamic LOD for v0 with clear
  diagnostics.
- Emit descriptor field loads from the texture uniform ABI:
  `ptr`, `width`, `height`, `row_stride`.
- Implement coordinate handling and bounds policy for `texelFetch`.
  The plan should pin whether integer out-of-range is clamped or rejected for
  v0, consistent with the design's strict validation posture.
- Emit format-specialized storage loads and conversion to shader float/Q32
  values for:
  - `R16Unorm`,
  - `Rgb16Unorm`,
  - `Rgba16Unorm`.
- Return GLSL-compatible `vec4` sample values:
  - R formats fill missing channels appropriately,
  - RGB formats return alpha `1.0`,
  - RGBA formats return all four channels.
- Validate runtime texture values against compile-time `TextureBindingSpec`
  before rendering/calling.
- Add filetests that pass on relevant backends (`wasm.q32`, `rv32c.q32`,
  `rv32n.q32` as applicable).

### Out of scope

- `texture(sampler2D, vec2)` normalized-coordinate sampling.
- Linear filtering.
- Repeat/mirror-repeat wrap lowering for normalized sampling.
- Height-one palette helper beyond direct integer `texelFetch` tests.
- WGSL source input or wgpu execution.

## Key decisions

- `texelFetch` is the foundation operation because it proves the full
  descriptor/ABI/backend path with the smallest sampling semantics.
- Format dispatch is compile-time via `TextureBindingSpec`; no per-sample
  runtime format switch.
- `Rgb16Unorm` is supported on CPU even though it is not WebGPU-portable.

## Deliverables

- Frontend lowering for `texelFetch`.
- LPIR sequence/helpers for descriptor loads, offset calculation, storage loads,
  and unorm-to-Q32 conversion.
- Runtime validation for bound `LpsTextureBuf` values.
- Passing exact-value filetests for `R16Unorm`, `Rgb16Unorm`, and
  `Rgba16Unorm`.
- Negative tests for unsupported LOD and invalid bindings.

## Dependencies

- Depends on Milestone 1 for logical texture types and binding specs.
- Depends on Milestone 2 for texture fixture/filetest support.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: This milestone crosses frontend lowering, LPIR generation,
runtime validation, storage-format conversion, and multiple backends. The
out-of-range policy and conversion details need to be made explicit before
dispatch.

**Suggested chat opener:**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

