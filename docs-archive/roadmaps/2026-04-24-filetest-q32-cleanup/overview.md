# Filetest q32 Cleanup Roadmap — Overview

## Motivation / rationale

The filetest suite currently mixes three different kinds of red output:

- Tests that should never pass on q32 because they require IEEE f32,
  NaN/Inf, f16/f64, or bit reinterpretation.
- Tests that expose real bugs or missing intended q32 features.
- Tests whose expectations or harness behavior are stale or wrong.

That makes the suite noisy and weakens its value as a regression
signal. The goal is to turn the backlog into a clean contract:
unsupported q32 cases are explicitly skipped, known broken cases are
expected failures with unexpected-pass alerts, and each bug-fix
milestone removes a coherent set of markers.

The roadmap is grounded in the 2026-04-23 triage reports:

```text
docs/reports/2026-04-23-filetest-triage/
├── unsupported.md
└── broken.md
```

Those reports are a map, not a frozen manifest. Each milestone re-runs
the relevant filetests against the current tree before deciding what to
mark, fix, or remove.

## Architecture / design

The roadmap is a filetest hygiene and repair loop around the existing
annotation system:

```text
docs/reports/2026-04-23-filetest-triage/
├── unsupported.md
└── broken.md
        │
        ▼
M1 annotation sweep
├── @unsupported(...) for intrinsic no-real-f32 q32 exclusions
└── @broken(...) for intended q32 behavior that is currently failing
        │
        ▼
M2-M7 fix milestones
├── targeted filetest runs while developing
├── remove stale @broken markers as fixes land
└── just test-filetests per milestone
        │
        ▼
M8 reconciliation
├── no stale unexpected-pass markers
├── unsupported list still matches q32 product boundary
└── docs/design/q32.md updated where implementation clarified semantics
```

Key policy decisions:

- `@unsupported` means "not in q32 by design," across all q32 targets
  unless genuinely backend-specific.
- `@broken` means "intended to pass eventually," and is removed by the
  milestone that fixes it.
- Numeric fixes target intended q32 semantics. `docs/design/q32.md` is
  the starting reference, but each fix must sanity-check the doc against
  the reference `Q32` implementation and product backends.
- `global-future/*` is not treated as a broken q32 category; it is
  future product surface.

## Alternatives considered

- **Fix without annotating first.** Rejected because it leaves the suite
  noisy while longer milestones are in progress.
- **Treat rv32 as the absolute numeric spec.** Rejected. rv32 is the
  production path and a useful baseline, but q32 semantics are
  project-defined and may require reconciling docs, reference code, and
  backend behavior.
- **Keep wrong expectation fixes with their subsystem milestones.**
  Rejected for this roadmap. Moving them into the early quick-wins
  milestone reduces noise and keeps later subsystem work focused on
  implementation changes.
- **Fold real-f32 work into this roadmap.** Rejected. That changes the
  product boundary and should be a separate roadmap.

## Risks

- The triage report is a snapshot and may already be stale due to
  ongoing aggregate/frontend work.
- Some `broken.md` rows may be misclassified once re-run against the
  current tree.
- `docs/design/q32.md` may lag small implementation fixes, so numeric
  milestones must not blindly edit code to match stale docs.
- Frontend aggregate l-values, global stores, and control-flow
  aggregate copies may overlap more than the milestone names suggest.
- `@broken` markers can become stale if unrelated work fixes a test;
  unexpected-pass output is the mitigation.

## Scope estimate

Eight milestones:

- One annotation/baseline milestone.
- Six repair milestones grouped by root cause.
- One cleanup and validation milestone.
