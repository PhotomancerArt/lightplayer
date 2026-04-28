# Milestone 6: Integration validation and cleanup

## Goal

Validate texture access end-to-end across the `lp-shader` stack, clean up
temporary scaffolding, and document the final contract for downstream lpfx and
future wgpu work.

## Suggested plan location

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m6-integration-validation-cleanup/`

Small plan: `plan.md`.

## Scope

### In scope

- Run and document validation commands for:
  - texture filetests across supported targets,
  - `lp-shader` API tests,
  - relevant frontend/shared/runtime crate tests,
  - RV32 firmware/compiler checks required by repository policy when touching
    shader pipeline code.
- Add or update documentation that summarizes:
  - `TextureBindingSpec`,
  - `Texture2D` logical type,
  - uniform descriptor ABI,
  - filetest fixture syntax,
  - supported filters/wraps/formats,
  - deferred features and wgpu notes.
- Audit diagnostics for clarity and sampler-name context.
- Remove temporary TODOs, expect-fail markers, debug hooks, or duplicated helper
  code created during earlier milestones.
- Add a concise follow-up note for:
  - real wgpu comparison runner,
  - WGSL source input,
  - `clamp_to_border`,
  - mipmaps/manual LOD if a concrete effect requires them.

### Out of scope

- New sampling features.
- lpfx/domain integration beyond documentation and API readiness.
- wgpu backend implementation.
- Schema changes in external roadmaps.

## Key decisions

- The final milestone is validation and cleanup only.
- Future wgpu parity is documented as a follow-up, not claimed as completed by
  this roadmap.
- Deferred features remain explicit so later agents do not rediscover the same
  design questions.

## Deliverables

- Passing validation commands or documented blockers.
- Updated docs/comments for texture access APIs and filetest syntax.
- Removed temporary scaffolding.
- Follow-up notes for deferred texture/wgpu work.

## Dependencies

- Depends on Milestones 1-5, including the M3a/M3b/M3c `texelFetch` split.

## Execution strategy

**Option B — Small plan (`/plan-small`).**

Justification: This milestone has no new architecture. It needs a short
checklist-style plan to sequence validation, cleanup, and documentation updates
without reopening texture design.

**Suggested chat opener:**

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?

