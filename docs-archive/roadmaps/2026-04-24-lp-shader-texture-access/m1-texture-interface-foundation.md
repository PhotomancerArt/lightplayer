# Milestone 1: Texture interface foundation

## Goal

Establish the shared texture interface contract: logical `Texture2D` type,
compile-time binding specs, compile descriptor plumbing, and uniform descriptor
ABI. This milestone should not yet implement texture sampling behavior; it makes
texture uniforms visible, typed, and validated.

## Suggested plan location

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m1-texture-interface-foundation/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

### In scope

- Add shared texture binding vocabulary in `lps-shared`, near
  `TextureStorageFormat`:
  - `TextureBindingSpec`
  - `TextureFilter`
  - `TextureWrap`
  - `TextureShapeHint`
- Add a logical `Texture2D`/`sampler2D` type to `LpsType`.
- Define the guest ABI descriptor shape for texture uniforms:
  `{ ptr: u32, width: u32, height: u32, row_stride: u32 }`.
- Add typed runtime value/helper representation for texture uniforms, built
  from `LpsTextureBuf` rather than hand-authored raw pointer structs.
- Replace or augment `LpsEngine::compile_px` with a named compile descriptor
  that carries source, output format, compiler config, and texture binding
  specs.
- Extend frontend metadata extraction so GLSL `sampler2D` uniforms are visible
  as logical texture uniforms and can be matched against binding specs.
- Add validation for compile-time texture interface mismatches:
  - shader declares sampler with no spec,
  - spec names nonexistent sampler,
  - unsupported source type or sampler shape.

### Out of scope

- `texelFetch` or `texture` lowering.
- Texture fixture syntax in filetests beyond whatever minimal parser stubs are
  needed for compile-time validation tests.
- Runtime sampling behavior.
- WGSL source input.
- lpfx/domain schema changes.

## Key decisions

- `Texture2D` is a logical shader type. It must not be exposed in metadata or
  diagnostics as a plain struct even though its ABI lowers to a uniform
  descriptor.
- Texture policy comes from `TextureBindingSpec`, keyed by sampler uniform
  name, not from GLSL layout qualifiers or LP-specific function names.
- Validation is strict and fail-fast.
- The ABI descriptor includes `row_stride` in v0.

## Deliverables

- Shared texture binding types and logical texture type.
- Compile descriptor API shape for `compile_px` texture specs.
- Metadata/validation support for GLSL `sampler2D` declarations.
- Texture uniform ABI layout documented in code comments/tests.
- Unit tests for interface validation and ABI layout.

## Dependencies

- Depends on `docs/design/lp-shader-texture-access.md`.
- Does not depend on later milestones.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: This milestone touches shared type definitions, metadata
extraction, compile API shape, uniform ABI layout, and validation. It needs a
full plan to preserve compatibility with existing uniform/global layout behavior
and avoid leaking ABI structs into public metadata.

**Suggested chat opener:**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

