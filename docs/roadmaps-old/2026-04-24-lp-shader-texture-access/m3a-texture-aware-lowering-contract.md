# Milestone 3a: Texture-aware lowering contract

## Goal

Make `lps-frontend` texture-spec aware and prove the `texelFetch` lowering
contract before implementing the full storage load path.

This milestone is the frontend/control-plane slice of the original
`texelFetch` milestone. It should answer the hard shape questions up front:
what Naga emits for GLSL `texelFetch`, how lowering receives
`TextureBindingSpec` metadata, how sampler names are recovered, and how
unsupported LOD/operand cases are diagnosed.

## Suggested plan location

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

### In scope

- Add a texture-spec-aware frontend lowering entry point.
- Keep the existing texture-free `lps_frontend::lower(&NagaModule)` API as a
  compatibility wrapper.
- Confirm and document Naga's representation of GLSL
  `texelFetch(sampler2D, ivec2, lod)`.
- Resolve texture operands back to direct uniform sampler names so lowering can
  find the matching `TextureBindingSpec`.
- Reject unsupported `texelFetch` forms with clear sampler/function context:
  - nonzero literal LOD,
  - dynamic LOD,
  - non-`Texture2D` operand,
  - texture values passed through locals or function parameters if the operand
    can no longer be resolved to a direct uniform.
- Add narrow diagnostics/filetests or unit tests proving the lowering contract.
- Decide where compile-time texture specs are stored for later runtime
  validation, if that storage is needed by downstream milestones.

### Out of scope

- Full texel address calculation and storage loads.
- Format-specialized `Load16U` / `Unorm16toF` codegen.
- Runtime `LpsTextureBuf` validation.
- Backend exact-value texture filetests.
- `texture(sampler2D, vec2)` normalized-coordinate sampling.

## Key decisions

- Split from the original M3 because frontend metadata and Naga IR shape are a
  distinct risk from the actual fetch math.
- Format dispatch still comes from `TextureBindingSpec`; this milestone only
  makes that metadata available at the lowering point.
- The v0 contract stays strict: `texelFetch` is only supported when the texture
  operand resolves to a declared sampler uniform with a matching spec.

## Deliverables

- A spec-aware frontend lowering API.
- Documented/verified Naga IR shape for `texelFetch`.
- Clear lowering diagnostics for unsupported texture operands and LOD.
- Tests or filetests that fail for the intended unsupported forms and prove the
  metadata lookup path.
- Updated notes for M3b about the exact helper/API surface it should build on.

## Dependencies

- Depends on Milestone 1 for logical texture types and binding specs.
- Depends on Milestone 2 for texture fixture/filetest syntax and diagnostics.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: this milestone changes the frontend lowering contract and should
settle Naga/source-shape questions before backend/codegen work starts.

**Suggested chat opener:**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

