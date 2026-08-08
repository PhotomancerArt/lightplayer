---
status: fixed
found: 2026-08-08      # how: ci (validate-stories ping-pong after PR #386 merged)
fixed: 003f068bf
area: lpa-studio-web story capture (clock-face + panel-state transport stories)
class: stale-measurement
related:
  - 2026-08-05-clock-face-baselines-oscillate.md
  - 2026-08-05-popover-line-parked-on-a-rounding-tie.md
  - ../debt/story-capture-pipeline.md
  - ../adr/2026-07-26-ci-story-auto-commit.md
---
# Tape transport canvas repeats the clock-face oscillation, one bot commit per CI run

**Symptom** — Since PR #386 (the clock transport panel) merged, the
`validate-stories` auto-commit fired after EVERY CI run on every branch,
moving `studio__node__clock-face__*` and
`studio__module__panel-state__transport*` baselines **back and forth
between two byte states** (e.g. `transport-off-live__md` ping-ponged
14726 ↔ 14320 across four consecutive bot commits). Because bot pushes
don't trigger workflows, branch heads chased baselines and never showed
green checks.

**Root cause** — The same mechanism as
2026-08-05-clock-face-baselines-oscillate, reached through the NEW canvas
PR #386 added. `TapeTransportDriver::paint` sizes the backing store from
`getBoundingClientRect()`, and on a frozen story page the driver latches
`frozen` and stops the rAF loop after the first frame. The app's
stylesheet is injected by the wasm bundle after boot, so a paint that
beats it measures the canvas's unstyled box and bakes a bitmap the
browser then squeezes into the real 62px-tall one. Both outcomes are
stable terminals; the baseline records whichever one that run reached.
The pixel diff of the two committed variants of `clock-face__fast__lg`
is confined to the tape canvas band (y 134–194), the tape-canvas
signature of the earlier defect's trace-canvas one.

The 2026-08-05 fix had three parts — inline box style,
`ux-box-sized-canvas` under the capture's ready gate, and a
`ResizeObserver` repaint — and the tape driver, written months of
context later as "phasor_trace's contract to the letter", carried the
freeze contract but **none of the three geometry guards**. The ready
gate only asserts the backing-store invariant for canvases that opt in
via the class, so the unmarked canvas sailed past it.

**Fix** — The same three parts, applied to the tape canvas:

1. `TapeTransportDriver` installs a `ResizeObserver` over the tape canvas
   and repaints on box change (idempotent: time pins to the anchor on a
   frozen page).
2. The canvas's box moved to an inline `style`
   (`display:block;width:100%;height:62px`), applied on first layout and
   independent of the `width`/`height` attributes the paint writes.
3. The canvas wears `ux-box-sized-canvas`, so the capture's ready gate
   refuses to shoot while its backing store disagrees with its box.

**Verified** — two consecutive local captures
(`STUDIO_STORY_PNGS_CONCURRENCY=1 just studio-story-pngs clock-face
panel-state`) produce byte-identical PNGs.

**Lesson** — A defect fixed in one component recurs when the mechanism is
re-implemented elsewhere; the class guard (`ux-box-sized-canvas` + gate)
only protects canvases that opt in. Any NEW imperatively-painted canvas
that stops repainting to be photographed must ship all three guards, not
just the time pin — "paint depends on two inputs, time and geometry, and
both must be pinned" (the earlier entry's deeper lesson, now violated
once and re-learned). Grep check when adding such a canvas:
`ux-box-sized-canvas` must appear beside every `data-preview-painted`
writer whose loop can stop.
