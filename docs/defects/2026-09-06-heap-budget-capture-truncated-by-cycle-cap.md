---
status: fixed
found: 2026-09-06      # ci (heap-budget ratchet, PR #524)
fixed: this change
area: scripts/heap-budget-check.sh (lp-cli profile startup gate)
class: nondeterministic-capture
related: [docs/heap-budget-gate.md, docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md, lp2025/2026-09-06-0047-scale-dependent-smoothing]
---
# The heap-budget startup capture is cut by the cycle cap mid-compile, and records the cut as a figure

**Symptom** — PR #524 (a 24 B field on the output node, no change to any
compile path) turned the ratchet red on `examples/meteor (startup)
shader-compile.transient grew: 32274 > recorded 0` and `largest_alloc grew:
8148 > recorded 0`, with `alloc_count` and `alloc_bytes` still 0 — a window
with a transient but no allocations. On the branch's previous commit the
same cells measured 0, on CI and locally alike.

**Root cause** — `lp-cli profile` stops at `--max-cycles` (default
200,000,000) and the gate script never passed a larger one nor looked at
`meta.json`'s `terminated_by`. Meteor's startup capture does not fit: the
compile begins ~165M cycles in (frame 2 opens at ~128M) and needs well past
200M, so every meteor startup capture on record ended with the
`shader-compile` window OPEN. An open window's figures are whatever the
collector had when the cap fell — 0 when the cap landed early in the
compile (the recorded baseline), partial numbers when it landed later.
The ~3M-cycle timing shift of an unrelated change moved the cap from one
regime to the other. The `frame` window's second opening was cut the same
way, so meteor's recorded cold-start `frame` figures were frame 1 only —
never the "frame that contains the compile" the mode documents. The
profiler's own `warning: --max-cycles reached` went to the stderr the
script discards.

**Fix** — `scripts/heap-budget-check.sh` passes `--max-cycles 400000000`
(`MAX_CYCLES`) to every session and REFUSES a capture whose `meta.json`
says `terminated_by: max_cycles`, in `check` and `baseline` alike, naming
the knob to raise. The record is re-baselined with meteor's startup
capture reaching the end of frame 2 for the first time (its
`shader-compile` and `frame` figures are now real cold-start numbers, not
truncation artifacts).

**Regression coverage** — the refusal itself: a capture that hits the cap
fails the gate loudly instead of recording. No unit test; the script is a
`jq` pipeline over profiler output.

**Lesson** — a capture bound by a budget must say so in-band, and the
consumer must check. Both halves existed (the profiler wrote
`terminated_by`, printed a warning) and neither was read: the stderr was
discarded and the JSON field ignored, so a truncated measurement was
indistinguishable from a complete one for a month. A ratchet that can
record artifacts will eventually ratchet against them.
