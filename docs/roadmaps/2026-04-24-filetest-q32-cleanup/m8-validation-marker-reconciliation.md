# Milestone 8: Validation and Marker Reconciliation

## Goal

Finish the roadmap by reconciling filetest markers, documentation, and
validation output across the full q32 matrix.

## Suggested plan location

Implementation plan:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m8-validation-marker-reconciliation/`

Use `/plan-small` to produce:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m8-validation-marker-reconciliation/plan.md`

## Scope

In scope:

- Run a full `just test-filetests` sweep.
- Run targeted spot checks for any file groups that changed during the
  roadmap.
- Remove stale `@broken` markers that now unexpected-pass.
- Verify that remaining `@unsupported` markers still match the q32
  product boundary.
- Verify no `global-future/*` cases were accidentally folded into the
  q32 broken backlog.
- Update roadmap notes or adjacent docs with final marker counts and
  any deferred follow-up work.
- Confirm `docs/design/q32.md` reflects any semantics clarified during
  numeric fixes.

Out of scope:

- Large new feature work discovered during final validation.
- Real IEEE f32 implementation.
- Performance optimization beyond cleanup of temporary scaffolding.

## Key decisions

- M8 is a reconciliation milestone, not the first full validation run.
  Every earlier milestone already ran `just test-filetests`.
- Remaining unsupported markers should be explainable as q32 product
  boundaries, not as hidden bugs.
- Any newly discovered large failure becomes a new report or roadmap,
  not scope creep inside final cleanup.

## Deliverables

- Full filetest matrix passing at the intended baseline.
- No stale unexpected-pass markers.
- Final docs update summarizing what remains unsupported on q32 and why.
- Any small cleanup of temporary report artifacts introduced during the
  roadmap.

## Dependencies

- Milestones 1 through 7.

## Execution Strategy

**Option B — Small plan (`/plan-small`).**

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?

Small plan: the work is validation and cleanup with clear commands, but
it needs a checklist to avoid missing stale markers or doc drift.
