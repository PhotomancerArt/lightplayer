# Milestone 1: Annotation Baseline

## Goal

Establish a clean filetest baseline by marking all current triage items
with the right expectation: `@unsupported` for q32-excluded behavior and
`@broken` for intended q32 behavior that currently fails.

## Suggested plan location

Implementation plan:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m1-annotation-baseline/`

Use `/plan-small` to produce:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m1-annotation-baseline/plan.md`

## Scope

In scope:

- Re-run the relevant files from `docs/reports/2026-04-23-filetest-triage/`
  against the current tree before annotating.
- Add `// @unsupported(jit.q32)`, `// @unsupported(wasm.q32)`,
  `// @unsupported(rv32c.q32)`, and `// @unsupported(rv32n.q32)` to
  tests that are intrinsically outside q32 semantics.
- Add `// @broken(<target>)` to intended q32 behavior that currently
  fails, preserving target-specific shape where failures are genuinely
  target-specific.
- Keep `global-future/*` out of the broken-fix backlog; document its
  policy outcome if it appears in validation output.
- Run `just test-filetests` and confirm the suite is green except for
  expected skipped/expected-failure accounting.

Out of scope:

- Fixing implementation bugs.
- Changing q32 semantics.
- Adding real IEEE f32 support.
- Rewriting filetest infrastructure beyond small annotation handling
  fixes discovered during the sweep.

## Key decisions

- Mark first, then fix. This milestone creates the baseline that later
  milestones retire.
- Intrinsic no-real-f32 exclusions are unsupported on all q32 targets,
  not just the target that exposed the failure in the 2026-04-23
  snapshot.
- `@broken` is for intended q32 behavior that should eventually pass
  and therefore should produce unexpected-pass alerts when stale.

## Deliverables

- Updated filetests with `@unsupported` markers for the `unsupported.md`
  corpus.
- Updated filetests with `@broken` markers for the current `broken.md`
  corpus.
- Any small report/doc note needed to explain entries that changed shape
  since the snapshot.
- A green `just test-filetests` baseline, with unsupported and expected
  broken counts understood.

## Dependencies

- The existing annotation parser and runner support for
  `@unsupported` / `@broken`.
- The triage reports in
  `docs/reports/2026-04-23-filetest-triage/`.

## Execution Strategy

**Option B — Small plan (`/plan-small`).**

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?

Small plan: the work is mostly mechanical but touches many test files
and needs a current-tree validation pass before applying snapshot-based
markers.
