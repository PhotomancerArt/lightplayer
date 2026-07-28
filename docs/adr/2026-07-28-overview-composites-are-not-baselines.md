# ADR: Generated `overview` composites are not pixel baselines

- **Status:** Accepted
- **Date:** 2026-07-28
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The story book synthesizes one `overview` page per component that stacks
every story of that component. Since the CI-canonical capture cutover
(`2026-07-26-ci-canonical-story-capture.md`) those pages have been
captured and committed like any other story: 153 of 1053 baseline PNGs,
but **27 MB of the set's 55 MB** and the tallest pages by an order of
magnitude — 10 000 to 25 000 px against a 760 px capture viewport, where
no other story exceeds 3400.

They flip-flopped. `docs/defects/2026-07-28-overview-composite-capture-races.md`
records the mechanism: `Page.captureScreenshot` with
`captureBeyondViewport: true` does not reliably paint composited effects
that far below the fold, so the device card's `backdrop-filter` overlays
came out with their blur applied and their own content missing — at every
overlay story in the page, and nowhere in the first 5658 px. Both
renderings are stable within a page load, so the capture's stable-pair
check passes on either one and commits a coin flip. Because each
auto-refresh commit retriggers CI, and CI flips it back, the result is an
unbounded churn loop.

The relevant asymmetry: a composite is *derived*. Every state it shows is
already captured on its own page, at a size the capture path handles. It
can only ever report drift its member baselines already report — while
contributing the one page shape that breaks the capture.

## Decision

Generated `overview` composites are excluded from the story-capture
pipeline. `discoverStoryIds()` in `scripts/studio-story-pngs.mjs` drops
ids ending in `/overview`; the 153 composite baselines are deleted.

The composite pages themselves stay — they are a good way to browse a
component's states in the story book. They are simply not pixel
baselines.

`/overview` is now a reserved id suffix. `component_overview_id` in
`stories/story_book.rs` owns it, and
`overview_ids_are_reserved_for_generated_composites` fails the build if an
authored `#[story]` claims it. (It found one on the first run:
`project-opening-frame`'s only story was named `overview`, so the
composite had been shadowing it — the authored page was unreachable and
its "baseline" was a composite capture.)

## Consequences

- The churn loop is gone at its source, without touching the pixel
  tolerances (`docs/debt/story-capture-pipeline.md` forbids widening them).
- The baseline set halves in bytes and the capture pass loses its heaviest
  pages — the family that historically wedged capture runs.
- `base__code-editor__overview__*` was the de facto detector for
  `2026-07-27-code-editor-gutter-misaligned.md`. That detector is gone,
  and it was always a lottery (it caught the bug by flipping). The
  assertion-based check that defect already asks for is now the only route
  to coverage; its entry says so.
- The beyond-viewport capture path is still used by 135 non-composite
  baselines, all ≤3400 px, none ever observed failing this way. If one
  ever does, revisit the capture path itself rather than this decision.
- Story capture and the story book now disagree about what a story id
  means. The suffix is the seam, documented on both sides and pinned by a
  test.

## Alternatives Considered

- **Disable CSS transitions in story-png mode** (proposed in the defect as
  filed). Rejected: already implemented since 250ea7ff7, and not the
  mechanism. `backdrop-filter` is a static rule here and the missing
  content is the overlay's own paint, not an interpolated value.
- **Capture tall pages at a full-height emulated viewport**
  (`setDeviceMetricsOverride` to the content height, no
  `captureBeyondViewport`). This is the "real" fix and would cover the 135
  tall non-composite stories too. Rejected for now: it re-baselines every
  tall story, a 15 000 px viewport may hit surface limits and trade one
  artifact class for another, and the flip is not locally reproducible —
  so the change could not be verified before landing. Composites are the
  only page shape observed to fail; removing them is verifiable and
  reversible. Revisit if a non-composite story ever flips this way.
- **Neutralize `backdrop-filter` during capture.** Rejected: it would cost
  the *per-story* overlay baselines their blur — real design coverage —
  to fix pages that carry no coverage at all.
- **Keep capturing composites as a CI artifact without committing them.**
  Rejected: without a committed baseline there is nothing to compare
  against, so it buys no signal for the capture cost.

## Follow-ups

- The renamed `project-opening-frame/default` story has no baseline yet;
  the first `validate-stories` run adds it via the normal auto-commit
  path.
- `2026-07-27-code-editor-gutter-misaligned.md` now needs its assertion
  test to have any regression coverage at all.
