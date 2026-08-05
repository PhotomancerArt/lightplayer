---
status: carried
since: 2026-08-05      # count-bounded gradient storage landed; the maximal case remains
logged: 2026-08-05
area: lpc-wire/project-read
related:
  [
    "../design/color.md",
    "lp-core/lpc-shared/tests/gradient_wire_size.rs",
  ]
---
# A maximal gradient cycle still exceeds one project-read frame

**Shape** — Count-bounded `GradientConfig` storage (color.md §5) made
every realistic palette a small fraction of the 16 KiB project-read
frame budget, and a full cycle of WLED-scale imports (8 × 18 stops)
fits a frame alone — both pinned by
`lp-core/lpc-shared/tests/gradient_wire_size.rs`. But the maximal
LEGAL config — 8 members × 24 authored stops, every stop meaningful —
measures ~21 KiB of wire `LpValue` JSON, larger than any single frame.
An event echoing one (the binding-graph probe's channel values after a
panel pick, a def slot-root snapshot after a slot-local pick) would
fail the whole project read with "project-read event exceeded frame
budget", exactly the failure the padded form produced for EVERY
config.

**Carrying cost** — A user who builds a large-enough cycle (roughly:
more than ~14 KiB of authored stops in one config, plus whatever else
rides the same event) gets a project that stops syncing with no path
back except editing files by hand. Nothing in the chooser or the
model's `validate()` warns at authoring time; the failure appears at
the next read, far from the gesture that caused it.

**Retirement** — Either (a) generic oversized-event chunking: extend
the probe/slot-root emit path with the serialized-bytes chunking the
pixel-buffer probes already use (`ProjectReadProbeEvent::ResultBegin`
/ `ResultBytes` / `ResultEnd`), so any single event too large for a
frame streams instead of failing; or (b) a denser stop encoding on the
wire (the §5 recipe's per-stop struct costs ~110 wire bytes; a
vec4-per-stop packing would be ~4× denser but changes the documented
recipe shape). (a) is the general cure — it also covers big defs that
have nothing to do with palettes.
