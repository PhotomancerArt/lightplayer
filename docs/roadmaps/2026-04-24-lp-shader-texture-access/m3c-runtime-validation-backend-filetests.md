# Milestone 3c: Runtime validation and backend filetests

## Goal

Make `texelFetch` merge-ready as a product-facing capability by validating
runtime texture bindings and proving exact fetch behavior across relevant LPVM
backends.

This milestone is the validation/integration slice of the original M3. It
builds on the M3b fetch machinery and closes the gaps that are not part of core
codegen: retaining compile-time specs where runtime needs them, checking
bound texture values, and exercising the backend matrix with filetests.

## Suggested plan location

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m3c-runtime-validation-backend-filetests/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

### In scope

- Retain compile-time texture specs in the compiled shader/runtime metadata if
  they are not already retained by M3a/M3b.
- Validate runtime texture bindings against `TextureBindingSpec` before render
  or call execution where the public runtime has enough information:
  - missing texture binding,
  - format mismatch,
  - `TextureShapeHint::HeightOne` with runtime `height != 1`,
  - malformed descriptors when they can be detected without unsafe probing.
- Keep typed `LpsValueF32::Texture2D` / `LpsValueQ32::Texture2D` as the runtime
  binding surface; do not reintroduce raw `UVec4` descriptor writes.
- Add exact-value filetests for `texelFetch` on:
  - `wasm.q32`,
  - `rv32c.q32`,
  - `rv32n.q32` as applicable.
- Cover all v0 storage formats: `R16Unorm`, `Rgb16Unorm`, and `Rgba16Unorm`.
- Add negative filetests for invalid runtime binding/spec scenarios not already
  covered by M2/M3a/M3b.
- Replace or update the M2 design-doc-only fixture example so texture fixtures
  include at least one real `texelFetch` behavior test.

### Out of scope

- New sampling semantics.
- Product-level lpfx/lp-domain texture routing.
- Public palette/height-one helper APIs beyond validation needed for
  `TextureShapeHint::HeightOne`.
- wgpu execution parity.
- New texture formats.

## Key decisions

- Runtime validation is part of the fetch foundation, not deferred to filtered
  sampling, because bad bindings should fail before executing shader code.
- Filetests remain the primary cross-backend validation surface for exact
  storage-to-`vec4` behavior.
- The host API should continue to traffic in typed texture values/descriptors,
  not raw descriptor-shaped vectors.

## Deliverables

- Runtime validation for texture bindings available through the public
  `lp-shader` path.
- Backend filetests that pass for exact `texelFetch` values on supported
  q32 targets.
- Negative diagnostics for missing, mismatched, or shape-invalid runtime
  texture inputs.
- Updated M2/M3 documentation notes reflecting that fixtures now execute real
  texture reads.

## Dependencies

- Depends on Milestone 3b for working `texelFetch` lowering and format-specific
  loads.
- Depends on Milestone 2 for texture fixture parsing/allocation.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: this milestone crosses public runtime behavior, diagnostics, and
the backend test matrix. It should be its own commit-sized unit after codegen is
stable.

**Suggested chat opener:**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

