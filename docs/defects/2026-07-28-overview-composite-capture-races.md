---
status: fixed
found: 2026-07-28      # how: ci (PR #163 baseline auto-refresh ping-pong)
fixed: this change
area: lpa-studio-web story capture — auto-generated `overview` composite stories
class: nondeterministic-capture
related:
  - 2026-07-27-code-editor-gutter-misaligned.md
  - 2026-07-27-story-check-tolerance-ignores-amplitude.md
  - ../debt/story-capture-pipeline.md
  - ../adr/2026-07-28-overview-composites-are-not-baselines.md
---
# Overview composite captures race in-page transitions and flip-flop

**Symptom** — On PR #163, consecutive `validate-stories` runs ping-ponged
`studio__roster__roster-card__overview__lg.png` between two renderings
(766823 ↔ 775606 bytes, commits 2201e9041 → 05eed3900), immediately after
`studio__home__home-gallery__opening-a-project__sm.png` had ping-ponged the
same way (4714e8789 → 4529c52c8, that one since fixed by the
`StaticThumbPreviews` story gate). `base__code-editor__overview__sm.png`
flipped in the same window (01f324923). Every auto-refresh commit
retriggers CI, whose capture flips a composite again — an unbounded churn
loop as long as runs keep being approved.

**Root cause** — Not a transition race. The two committed variants were
diffed pixel-for-pixel, and the mechanism is narrower and louder than the
title of this entry (kept as filed) suggests:

- The diff is **279448 px, max Δ243**, confined to **one column band**
  (x 31–282) and **five row bands** (y 5658–6037, then 10705–10944,
  11169–11408, 11633–11872, 12097–12336).
- Those five bands are **exactly the stories in the composite whose card
  mounts a `backdrop-filter` layer** — `.ux-card-sheet` (blur 1.5px) and
  `.ux-card-op` (blur 3px). No other story in the page differs at all.
- In the bad variant those overlays render **with their blur applied but
  without their own paint**: the card body behind them is visibly blurred
  and the title bar above them is sharp (so the element is laid out,
  composited, and filtering its backdrop), while the overlay's background
  tint, label, progress bar, terminal, and panel are simply absent. That
  is a partly rasterized render surface, not an unfinished animation.
- Every affected band sits **7×–16× below the 760px capture viewport**.
  Everything above y=5658 is byte-identical between the two variants.

The composite page shape is what makes this reachable.
`Page.captureScreenshot` is called with `captureBeyondViewport: true`
against a 1080×**14991** clip; composited effects that far below the fold
are not reliably painted into the expanded surface. Non-composite stories
top out at 3400px, which is why the same card states capture stably on
their own pages.

The stable pair does not catch it because the two terminals are
**per-page-load**, not per-frame: whichever way a load resolves, both
shots agree, the pair passes, and the coin flip gets committed.

**Ruled out — the transitions hypothesis this entry was filed with.** The
capture already injects
`* { transition: none !important; animation: none !important }` before the
app mounts (`createCapturePage`, since 250ea7ff7). Nothing in the diffed
region is transition-driven: `backdrop-filter` here is a static rule, and
what is missing is the overlay's own paint, not an interpolated value.
`prefers-reduced-motion` emulation would have changed nothing.

**Also not this defect: the code-editor flip.** `base__code-editor__overview__sm`
diffs at 1355 px in columns 13–58 — the line-number gutter — which is
[code-editor-gutter-misaligned](2026-07-27-code-editor-gutter-misaligned.md),
a real app bug that is still open. It surfaced through a composite but is
a different mechanism; the "class signal" this entry was filed with (both
flip-floppers are composites) over-generalized from one shared symptom.

**Fix** — Generated `overview` composites are no longer captured or
committed. `discoverStoryIds()` drops story ids ending in `/overview` (the
suffix the story book synthesizes at `component_overview_id`), and the 153
composite baselines are deleted — 14.5% of the files but **27 MB of the
set's 55 MB**, and the heaviest pages in a capture pass with a documented
history of wedging on heavy pages. They carried no coverage the per-story
captures don't: a composite is a stack of states each already captured on
its own page. They stay browsable in the story book. Rationale and the
alternatives weighed:
[ADR 2026-07-28](../adr/2026-07-28-overview-composites-are-not-baselines.md).

Reserving the suffix immediately exposed a second, pre-existing defect:
`studio/home/project-opening-frame` had an authored `#[story] fn overview()`,
so its id collided with its own generated composite. `story_selection`
checks `overview_id` first, so **the composite shadowed the authored
story** — the story page was unreachable by URL, and its committed
"baseline" was a composite capture. That story is renamed `default`.

**Regression coverage** — `overview_ids_are_reserved_for_generated_composites`
(`stories/story_book.rs`) asserts that no `#[story]` id ends in `/overview`
and that every generated composite id does. It is what found the shadowed
story. `just test-rust` now runs `-p lpa-studio-web` with
`--features lpa-studio-web/stories`; without that the whole stories module
is not compiled and its tests silently do not run.

Nothing covers the Chrome behaviour itself: the fix removes the page shape
that reaches it rather than the beyond-viewport capture path, which 135
non-composite baselines still use (all ≤3400px, none ever observed
flipping this way).

**Lesson** — Two. First, this entry named a mechanism from the *look* of
the two renders ("mid-transition, blurred and faded") and proposed fixes
against it; the pixels said something else, and one of the two proposed
fixes was already implemented and irrelevant. Diff the bytes before
designing the fix — this pipeline has now produced several "obviously a
settling race" diagnoses that the pixels overturned, including the one
that turned out to be a real gutter bug. Second, an artifact *derived*
from other artifacts makes a bad regression baseline: a composite could
only ever report what its member stories already report, but it added a
page shape the capture path does not handle. When a baseline carries no
unique signal, its flakiness is pure cost.
