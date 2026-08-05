---
status: fixed
found: 2026-08-05      # how: ci
fixed: this change
area: lpa-studio-web story capture (clock-face stories)
class: stale-measurement
related:
  - 2026-07-28-overview-composite-capture-races.md
  - ../debt/story-capture-pipeline.md
  - ../adr/2026-07-26-ci-story-auto-commit.md
---
# Clock-face story baselines alternate between two renderings, one bot commit per CI run

**Symptom** — On PR #349 (a change touching no clock-face code), the
`validate-stories` auto-commit fired on two consecutive runs and moved the
same baselines **back and forth between two states**:

| story | main | after run 30984999205 | after run 30985988371 |
|---|---|---|---|
| `clock-face__crowd__lg` | `66a066b8` | `d7712f79` | `66a066b8` |
| `clock-face__shared__sm` | `cdd2a57b` | `f52a90a1` | `cdd2a57b` |

The third column is **byte-identical to main** — same blob hash, not merely
the same size. Both runs were green, and both were the pinned CI environment
the baselines are canonical for. `clock-face__default__md`,
`default__lg` and `shared__lg` moved too.

**Root cause** — A trace canvas keeps a bitmap drawn for a box it no longer
has. `paint_card` sizes the backing store from `getBoundingClientRect()`, and
on a frozen story page the driver latches `frozen` and stops the rAF loop
after the first frame — so the box measured by that one paint is the box the
bitmap is drawn for, forever. Studio's stylesheet is injected by the wasm
bundle *after* it boots (`index.html` says so in as many words), so a paint
that beats the stylesheet measures the canvas's UNSTYLED intrinsic size —
300×150, the HTML default — and the browser then squeezes that bitmap into
the real 42px-tall box. Nothing repaints it. Both outcomes are stable
terminals, the stable-pair capture passes on either, and the baseline records
whichever one that run reached.

The pixel diff of the two committed variants of `clock-face__crowd__lg`
(7 506 px, max Δ243, confined to the eight trace canvases — every other pixel
in the image is byte-identical) says exactly this, and rules out the settling
race the first read assumed:

- **The waveforms are not at different phases.** Peaks, troughs and square
  transitions land on the *same* output columns in both variants. Whatever
  differs, it is not time — which is what a paint racing the phase would have
  shown.
- **Every device-pixel-absolute constant shrinks by the same factor.** The
  trace's vertical pad (`3 * dpr` device px) reads 3.0 CSS px in one variant
  and ≈0.5 in the other; the stroke goes from 1.25 CSS px of ink to ≈0.24;
  the 1 px midline, drawn at 14% alpha, is **absent entirely** in the bad
  variant. A bitmap scaled down does that to absolute constants, and to a
  single-row feature it does it all-or-nothing — which is why the midline
  vanishes while the (vertical) square-wave risers survive at varying
  intensity. Normalized quantities — the curve itself — pass through
  unchanged, which is the other half of the same signature.
- The measurements fit backing 300×150 in a ≈130×42 box: predicted pad 0.84
  CSS px against 0.1–1.0 measured, and predicted stroke thinning and midline
  aliasing both observed. A canvas stretched to a *parent's* width instead
  (the other unstyled-layout candidate) is ruled out: it would have made the
  horizontal strokes thicker than the good variant, and they are thinner.

**Reproduced**, not just inferred: serving the story build from a static
server that delays the tailwind stylesheet by 1500 ms, the crowd story's eight
trace canvases painted at their pre-stylesheet box (backing 2160×84 for a box
that settles at 126.66×42) and stayed there. With the fix the same load
converges to 253×84 — the box × dpr — as soon as the page produces a frame.

**Verified**: a local capture of the clock-face stories with the fix matches
CI's *first* variant inside the trace-canvas band to a max channel delta of
**1** (against 243 for the other), so the run that produced `d7712f79` was the
one rendering correctly, and main currently holds the degraded bitmap. The
next CI capture refreshes those baselines once; the acceptance check is that
the run after it reports no drift.

**Fix** — Two changes to the same mechanism, plus a gate.

1. `PhasorTraceDriver` installs a `ResizeObserver` over every mounted card
   canvas and repaints on box change. Painting is idempotent (time is pinned
   to the card's anchor on a frozen page), so the extra pass only ever
   corrects geometry — and it runs even though the rAF loop has stopped,
   which is the property the frozen page needs.
2. The canvas's box moved from tailwind classes to an inline `style`. Inline
   declarations apply on the first layout, before the injected stylesheet —
   and, more importantly, they keep the box independent of the `width`/
   `height` attributes the paint writes. Without that, an unstyled canvas at
   `dpr > 1` takes its box *from* those attributes and the new repaint feeds
   itself, growing the element every frame until the stylesheet lands.
3. The capture's ready gate now refuses to shoot while any
   `canvas.ux-box-sized-canvas` has a backing store that disagrees with its
   box — the same idiom as the `data-preview-painted` and font gates.

**Regression coverage** — The ready gate above: it asserts the invariant this
defect violated on every capture of every story containing such a canvas, and
turns a repeat into a story-ready timeout (loud) instead of a bot commit
(silent). A unit test cannot reach this — the mechanism is layout timing in a
real browser. The structural gap the entry was filed against remains: nothing
compares two captures of the same tree, so a *different* nondeterministic
render would still churn silently.

**Lesson** — Auto-committing refreshed baselines converts a nondeterministic
capture from a loud failure into silent churn: every PR that trips the studio
path filter gets a bot commit, each one looking like ordinary drift, and the
branch never reaches a stable head. The ADR accepts "the green run sits one
commit behind the bot's" as a merge-time tradeoff, which is sound for *real*
drift; under oscillation that gap never closes, because the next run disagrees
with the one that just wrote. When judging whether a baseline refresh is
benign, **compare blob hashes against the previous refresh, not just against
main** — a hash that returns to an earlier value is the signature this class
leaves, and file sizes alone can hide it.

The deeper lesson is about the freeze. Pinning *time* made the paint
deterministic and stopping the loop made it cheap, but the paint depends on
two inputs, and only one of them was pinned: the geometry it measures is just
as much a race as the clock it reads. Any component that stops re-rendering
in order to be photographed has to have finished reading everything it
depends on — and on this app "the stylesheet has applied" is not among the
things a first paint may assume, because the stylesheet arrives after boot.
Components that cache geometry at mount are a named hazard class in the
story-capture debt entry (CodeMirror's height oracle, the popover's
`fonts.ready` re-measure); this is the same class reached through a canvas.
