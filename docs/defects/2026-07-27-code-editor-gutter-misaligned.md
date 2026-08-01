---
status: fixed
found: 2026-07-27      # how: ci (story-capture churn), then reproduced headlessly
fixed: 20875491f      # capture-side; see the Fix section for why that layer
area: lpa-studio-web/base/code_editor (vendored CodeMirror 6)
class: stale-measurement
related:
  - 2026-07-26-popover-outline-stale-on-content-resize.md
  - 2026-07-27-story-check-tolerance-ignores-amplitude.md
  - 2026-07-28-overview-composite-capture-races.md
  - ../debt/story-capture-pipeline.md
  - ../adr/2026-07-26-ci-canonical-story-capture.md
---
# Code editor line numbers walk out of alignment with their lines

**Symptom** — The line-number gutter advances **14px** per row while the code
lines advance **18px**, so each number sits progressively higher than the line
it labels — 36px adrift by line 9, with the lint marker beside the wrong line.
Measured directly off the two committed CI baselines of
`base__code-editor__overview__sm.png` (gutter digit tops, y-pixels):

| baseline | digit spacing | state |
| --- | --- | --- |
| `ccbdad901` (56635 B) | **18px** | aligned |
| `4e6580fcd` (56529 B) | **14px** | misaligned |

Both renders are reachable, which is why this surfaced as story-baseline churn
rather than as a bug report. **The misaligned render is the one currently
committed on main.**

**Root cause** — CodeMirror sizes gutter rows from `heightOracle.lineHeight`,
whose constructor default is a hardcoded `14`. The oracle is only populated by
measuring, and `ViewState.measure()` reads:

```js
let measureContent = refresh || this.mustMeasureContent || this.contentDOMHeight != domRect.height;
this.contentDOMHeight = domRect.height;   // consumed
this.mustMeasureContent = false;          // consumed
...
if (!this.inView && !this.scrollTarget && !inWindow(view.dom)) return 0;   // discarded
```

The re-measure signal is consumed at the top and then thrown away by the early
return whenever the editor is outside the window. An editor that loads **below
the fold** therefore never measures at all and keeps `lineHeight = 14`, while
the theme lays content out at `1.5 × 12px = 18px`.

Coming into view is the one thing that recovers it — `if (inView != this.inView)
{ this.inView = inView; if (inView) measureContent = true; }` runs before the
guard — so in the real app a user who scrolls to the editor sees it correct
itself. **In a story it never recovers**, because the story page does not
scroll (a `window.scrollTo` sweep moves nothing) and `captureBeyondViewport`
photographs the below-fold content anyway.

The bistability is a fold-boundary race: at the story viewport height of 760 the
editor lands at y≈590–844, straddling the fold, so a few pixels of layout
difference above it decide whether the first measure happens at all.

**Reproduction** — `lp-app/lpa-studio-web/scripts/gutter-alignment-probe.mjs`,
against a served release story build:

| viewport height | editor position | result |
| --- | --- | --- |
| 760 (story default) | top 590, in view | **10/10 aligned**, oracle 18 |
| 400 | top 410, below fold | **0/3 aligned**, oracle 14, 36px adrift |

macOS at the real story viewport does not reproduce it (the editor lands just
above the fold); CPU throttling to 8× does not change that. The viewport height
is the load-bearing variable, not machine speed.

**Fix** — the capture now grows its viewport to hold the whole story box
(`fitViewportToStory` in `studio-story-pngs.mjs`), so nothing is below the fold
and every lazily-measured widget measures for real. Verified with the probe:
below the fold, 0/2 runs aligned before and 2/2 after, oracle corrected 14 → 18
and the label offset down to 0.

Capture-side is the right layer here, which is worth stating because the
symptom looks like an app bug. In the app the editor recovers the moment it
scrolls into view, so a user sees at worst a flash; the story is the only place
the state is permanent, because a story page does not scroll. Fixing it in the
app would mean either overriding CodeMirror's row heights in CSS (the
alternative below — leaves a residual offset because the oracle is still wrong)
or reaching into CodeMirror internals to fake the geometry it refuses to
measure. Growing the viewport removes the condition instead of compensating for
it, and generalises to any widget that measures on first visibility.

The grown viewport is a measurement window, not the height anything is
photographed at — it is restored before the capture. That matters: a CI run of
the first version refreshed 26 surviving baselines, all tall stories, and the
change was pure growth. `studio-shell/simulator-ready` at sm went 3155px →
3246px with the first 3142 rows pixel-identical and 91px of empty space added,
because layout that sizes itself to the window expands with it. Restoring the
viewport keeps the corrected measurement (verified: the oracle stays 18 with
the editor back below the fold, 2/2 aligned) while leaving those baselines
untouched, so the change carries no baseline churn at all.

The alternative, measured and rejected:

1. **Pin the gutter row height in CSS** so it stops depending on whether a
   measurement happened:
   `.cm-lineNumbers .cm-gutterElement:not(:first-child), .cm-gutter-lint .cm-gutterElement { height: 1.5em !important }`
   (the lineNumbers gutter's first child is CodeMirror's hidden sizing spacer
   and must keep its own height; the lint gutter has no spacer). Verified
   **3/3 aligned** below the fold. Partial, though: it fixes the *walking
   drift* but leaves a constant **4px** offset (`maxLabelOffset` 4 vs 0 when
   the oracle is correct), because the oracle is still wrong. Safe only while
   `EditorView.lineWrapping` stays off — one line is then always exactly one
   line box.

Ruled out by testing, recorded so they are not retried: `view.requestMeasure()`,
a `window` resize event, an empty `dispatch({})`, a selection-only dispatch, and
moving `line-height` onto the theme root all leave the oracle at 14 — none of
them set `mustMeasureContent`, and none can get past the `inWindow` guard while
the editor is off screen. Forcing `mustMeasureContent` directly does not help
either, for the same reason. A `window.scrollTo` sweep in the capture is a no-op
because the story page is not scrollable.

**Not the same thing as the composite race** — `base__code-editor__overview__sm`
also appears in [overview-composite-capture-races](2026-07-28-overview-composite-capture-races.md),
which diagnoses composite flip-flops as a CSS transition captured mid-flight.
Two mechanisms on one story, distinguishable by the pixels: this one is a
discrete 18px↔14px change in gutter row spacing, not blurred or faded content.

**Regression coverage** — `gutter-alignment-probe.mjs` is the deterministic
check (exits non-zero on misalignment); run it at viewport height 400 to hold
the failing condition. Not yet wired into CI.

That probe is now the *only* route to coverage, not a nicer alternative to a
flaky one. `base__code-editor__overview__*` is what caught this by flipping, and
generated `overview` composites stopped being pixel baselines on 2026-07-28
(nondeterministic for the unrelated reason above). The surviving per-story
`base__code-editor__*` baselines sit in short stories where the editor is
already in view, so they froze the aligned terminal and will never report a
flip. Nothing in CI detects this defect today.

**Lesson** — Three things. A flaky visual baseline is evidence, not noise: this
was written off as capture churn across two sessions before anyone diffed the
pixels, and diffing them took minutes. An agent-driven browser pane is not a
valid measurement environment — its `requestAnimationFrame` never fires while
hidden, which fabricated both a "deterministic repro" and a "fix that works"
before headless Chrome contradicted both; measure UI timing in headless with
forced BeginFrames. And this is the second `stale-measurement` defect in two
days in this UI after the popover outline, both the same shape: something
measures once at mount and has no path back when the inputs to that measurement
change. Worth treating "cached at mount, never invalidated" as a known hazard
here rather than a fresh surprise each time.
