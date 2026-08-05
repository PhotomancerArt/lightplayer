---
status: carried
since: 2026-05-12      # the clock's f32 `accumulated_seconds` integrator
logged: 2026-08-04
area: lpc-engine clock/timebase + shader `seconds` uniforms
related:
  - "plan: ~/.photomancer/planning/lp2025/2026-08-03-2203-timeproduct-phasors/ (D4, notes.md 'f32 precision argument')"
  - "plan: ~/.photomancer/planning/lp2025/2026-08-04-0003-timeproduct-m2-core/ (D12 discovery, P5 sweep ledger)"
  - bounds-asserted-in-the-wrong-unit.md
---
# Unbounded seconds rot, and stop entirely after 9.1 hours in fixed mode

**Shape** — the clock integrates `accumulated_seconds` as an `f32` and
the `TimeProduct` hands that same number out through `seconds()`. An
unbounded seconds value has two independent long-runtime problems, and
both are properties of the *representation*, not of any one consumer:

- **f32 rot.** Absolute resolution scales with magnitude. At
  t ≈ 86 400 s (one day) an f32 ulp is 2⁻⁷ ≈ **7.8 ms** — marginal at
  60 fps after one day, visibly quantized after a few. Installations
  run for weeks. The clock's integrator has carried this since it was
  written; the TimeProduct inherits it verbatim.
- **A hard ceiling in fixed mode.** A `seconds` uniform reaching a
  `float_mode: "fixed"` shader is encoded Q16.16, and `q32_encode`
  **saturates** (`lp-shader/lps-q32/src/q32_encode.rs`). The range ends
  at ±32768 s = **9 h 6 min**, after which the uniform stops changing
  and the animation freezes with no error, no warning and no black
  frame. Every one of the ten `"kind": "seconds"` uniforms in the
  gallery today is on a fixed-mode shader.

**Phasors dodge both.** A phasor slot never carries elapsed time across
the boundary: the store integrates in f32 but hands over a wrapped
`[0,1)` phase plus a `u32` cycle count, so resolution is constant
forever and Q16.16 has room to spare (a u32 cycle counter is exact for
~2.2 years at 60 Hz). This is the main reason the M2 migration moved
every periodic animation onto phasors — the win is a real
device-overnight bug fixed, not only continuity feel. The residual risk
is exactly the surface phasors could **not** absorb: the sanctioned
integrators (`examples/meteor/sim.json`, `examples/events/event_{a,b}`)
and the noise-advance halves of the split conversions, all of which
genuinely want unbounded seconds.

⚠️ **Phasors dodge the carried magnitude, not the clock's own
resolution.** On hosts a phasor is evaluated as a closed-form function
of *effective seconds*, which is still f32: at t ≈ 10⁴ s a clock can
only name about a millisecond, so a short-period phasor's phase
quantizes at the same rate the seconds value does. The firmware
forward-integrator has the complementary version of the problem
(`φ += rate · Δt` accumulates its own error rather than inheriting the
magnitude). Neither is close to visible today, and both are fixed by
the same epoch rebase below — but "phasors are exact forever" is too
strong a claim to carry forward unqualified.

**Carrying cost** — a long-running install degrades and then stops
moving, and the failure is silent and slow enough that it is attributed
to anything but arithmetic (the whole class costs a debugging session
per discovery, and it cannot be reproduced without leaving a board on
overnight). It also constrains authoring: "declare Seconds and think
twice" is doctrine that has to be re-explained rather than enforced,
because nothing in the pipeline refuses a seconds uniform that will
outlive its own range.

**Workarounds**

- Prefer a phasor. If motion is periodic at all — even at a long
  period — a phasor slot is exact indefinitely; only true integrators
  and monotone counters need `seconds`. Note the caveat above: a
  phasor removes the *carried* magnitude, not the clock's own
  resolution.
- Where seconds are genuinely needed, integrate a **delta** and keep
  the accumulator inside the consumer's own state at whatever
  resolution it needs, rather than differencing two large seconds
  values (`examples/meteor/sim.glsl` is the shape to copy — it is
  still subject to the 9.1 h ceiling on its *input*, but it does not
  compound the f32 rot).
- To reproduce without waiting: write `transport.scrub_offset_seconds`
  on the clock card's Debug section (e.g. 32 000) and watch a
  fixed-mode `seconds` uniform flatten as it crosses the ceiling.

**Incident log**

- **2026-08-03 — f32 rot argued as the case for wrapped phase** (D4,
  `2026-08-03-2203-timeproduct-phasors/notes.md`). The 8 ms/day figure
  was the original reason the phasor value shape is wrapped rather
  than an unbounded ramp; the same paragraph flagged that raw
  `bus:time` already carried the debt and that it deserved an entry
  regardless of that plan's outcome.
- **2026-08-04 — the Q16.16 ceiling found during M2 discovery**
  (D12). Strictly worse than the rot story and previously unnoticed:
  fixed-mode `time` saturates after ~9.1 h, so a device left running
  overnight freezes its animation. Filed here rather than as a defect
  because no incident report exists — it was found by reading the
  encoder, and the migration removed most of the exposure before
  anyone hit it.

**Exit criteria** — one of:

1. The seconds surface stops being unbounded f32 end-to-end: the
   product's `seconds()` is rebased against a moving epoch (per query
   or per project load) so the magnitude a consumer sees stays small,
   with the epoch's discontinuity defined for consumers that difference
   across it. This is the cheap fix and it addresses both problems at
   once, because a rebased value never approaches either limit.
2. Or the host integrator moves to `f64` (devices stay f32) and the
   fixed-mode boundary refuses — rather than saturates — a seconds
   value outside Q16.16's range, so the failure is loud.

Either way, retire when a `seconds` uniform can be left running for a
week without its motion changing character, and when exceeding the
representable range is a diagnosable event rather than a freeze.
