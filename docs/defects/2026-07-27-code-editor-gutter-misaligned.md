---
status: open
found: 2026-07-27      # how: ci (story-capture churn), then reproduced live
area: lpa-studio-web/base/code_editor (vendored CodeMirror 6)
class: stale-measurement
related:
  - 2026-07-26-popover-outline-stale-on-content-resize.md
  - ../debt/story-capture-pipeline.md
  - ../adr/2026-07-26-ci-canonical-story-capture.md
---
# Code editor line numbers walk out of alignment with their lines

**Symptom** — In the code editor the line-number gutter advances **14px**
per row while the code lines advance **18px**, so each number sits
progressively higher than the line it labels — by line 9 the drift is
~36px, and the red lint marker for line 5 appears beside a different
line entirely. It is not always wrong: the same build renders correctly
some of the time, which is how it surfaced — as two CI story captures of
`base__code-editor__overview__sm` that differed by 1355 pixels
(489 above the check's significance threshold), one aligned and one not.

**Root cause** — CodeMirror sizes gutter elements from
`viewState.heightOracle.lineHeight`. That oracle caches **14** — `normal`
line-height at the theme's 12px — while the content lays out at **18**,
from `line-height: 1.5` on `.cm-scroller` in the studio theme
(`vendor-src/codemirror/entry.mjs`). The oracle is measured once and
keeps the stale value; the gutter then writes 14px inline heights against
18px lines. A genuine content re-measure *does* correct it to 18
(observed live), so whether a page ends up aligned depends on whether
anything forces one — making both the aligned and misaligned renders
stable end states. This is a theme-CSS/measurement ordering race, not the
webfont-metrics race first suspected: it reproduces with fonts fully
loaded and a freshly constructed editor.

**Fix** — none yet. Tested and **disproven** as fixes, each verified live
by reading `heightOracle.lineHeight` after the action:

| attempt | result |
| --- | --- |
| `view.requestMeasure()` | oracle stays 14 — does not re-read text size |
| `window` resize event | oracle stays 14 |
| empty `dispatch({})` | oracle stays 14 |
| selection-only dispatch | oracle stays 14 |
| `line-height: 1.5` moved onto the theme root `&` | root computes 18px, oracle still 14 |

What *did* correct it: mutating `contentDOM` (appending a probe
`.cm-line`) and a real document change — i.e. only something that forces
a true content re-measure. The next attempt should find the supported CM6
API for forcing a text-size re-measure after mount, or make the theme's
styles apply before the first measurement can run.

**Regression coverage** — none yet. The story baseline
`base__code-editor__overview__*` is the de facto detector: it is what
caught this. A fix should land with a check that gutter row advance
equals content line advance, so the assertion does not depend on the
capture pipeline noticing a flip.

**Lesson** — Two things. First, a flaky visual baseline is evidence, not
noise: this was written off as capture churn across two separate
sessions before anyone diffed the pixels, and it was a real user-visible
defect the whole time. A story that flips between two *stable* renders is
reporting that the app has two stable renders. Second, this is the second
`stale-measurement` defect in two days in this UI (after the popover
outline), and both share a shape: a component measures geometry once at
mount and has no path back to re-measure when the inputs to that
measurement settle later. Treat "cached at mount, never invalidated" as a
known hazard in this codebase rather than a surprise each time.
