# Milestone 2: Texture filetest fixtures and diagnostics

## Goal

Extend `lps-filetests` so texture resources, binding specs, fixture data, and
texture-specific diagnostics can be expressed in backend-neutral `.glsl`
filetests.

## Suggested plan location

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m2-texture-filetests/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

### In scope

- Add parser support for texture binding specs in comment directives, e.g.
  `// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d`.
- Add parser support for inline texture fixtures, e.g.
  `// texture-data: inputColor 3x1 rgba16unorm`.
- Define and implement pixel-grouped channel fixture syntax:
  - pixels separated by whitespace,
  - channels separated by commas with no spaces,
  - normalized float channels preferred for readability,
  - exact hex storage values allowed for precision/boundary cases.
- Allocate backend shared memory for fixtures and bind corresponding texture
  uniform descriptor values before each `// run:`.
- Add positive and negative diagnostic filetests for:
  - missing texture spec,
  - extra spec,
  - missing runtime fixture,
  - malformed fixture data,
  - format mismatch,
  - height-one promise mismatch,
  - unsupported filter/wrap spellings.
- Keep fixture declarations backend-neutral enough that a future wgpu runner can
  reuse them.

### Out of scope

- wgpu comparison runner.
- Large sidecar image fixtures.
- Actual `texelFetch`/`texture` execution behavior beyond stubs or
  expect-fail markers needed while later milestones are incomplete.
- Changes to lpfx/domain.

## Key decisions

- Inline fixtures are the v0 format; sidecars can be added later if larger test
  inputs become necessary.
- Float fixture channels are converted through canonical storage conversion.
  Hex fixture channels are exact stored values.
- Filetests are the primary validation surface for this roadmap.

## Deliverables

- Extended filetest parser model for texture specs and fixtures.
- Shared fixture encoder for `R16Unorm`, `Rgb16Unorm`, and `Rgba16Unorm`.
- Runtime fixture allocation/binding path for all existing LPVM filetest
  backends.
- Diagnostic tests for texture interface errors.
- Documentation/comments in the parser describing fixture syntax.

## Dependencies

- Depends on Milestone 1 for shared descriptor types and logical texture
  metadata.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: This milestone changes parser grammar, fixture encoding, backend
test harness setup, and diagnostic expectations. The exact directive grammar
and binding path should be pinned down before implementation.

**Suggested chat opener:**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

