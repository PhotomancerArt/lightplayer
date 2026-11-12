# 2026-04-20 Fluid Demo — Notes

## Scope of work

Build a hardware demo of an MSAFluid (Stam-style) fluid simulator running on
the esp32c6, driving the basic-fixture circular display (9 concentric rings,
241 RGB lamps). The demo should be visually compelling — that is, look like a
real lp2014-style fluid pulser — and let us turn knobs (resolution, solver
iterations, sampling mode, solver Hz, temporal interpolation on/off) to find
out what's actually playable on this hardware.

This is the next step after the mono perf experiment in
`docs/plans/2026-04-20-fluid-perf-experiment/` — we now have data showing
fluid is right at the edge of feasible, and we want to find out empirically
whether it crosses the line into "looks good" with the right tradeoffs.

A successful outcome here lets us answer the architecture question raised in
the parent thread: **should lpfx grow "texture-level Rust builtins"** —
stateful effect modules that produce a texture imperatively without GLSL —
or are shader-based effects sufficient? The demo we're building is
essentially a prototype of what such a builtin would look like.

### In scope

- RGB extension of `MsaFluidSolver` (currently mono).
- Port of `FluidPulser` 3-emitter time-oscillating pattern (lp2014 reference
  in `~/dev/personal/lightPlayer/PlayerCore/src/main/java/com/lightatplay/
  lightplayer/program/fluid/FluidPulser.kt`).
- Port of `emitCircleWithTarget` / `emitCircleWithAngle` / `emitDirectional`
  primitives (lp2014 reference in `…/rendering/FluidRenderer.kt`).
- Sampler from the fluid grid through the basic-fixture geometry (9
  concentric rings, 241 lamp positions in [0,1]²).
- WS2812 output via RMT on gpio4 (matches `examples/basic` wiring).
- Reuse of `lp_shared::DisplayPipeline` for temporal interpolation,
  LUT, and dither between solver frames and display frames.
- New `test_fluid_demo` cargo feature in `fw-esp32`, parallel to
  `test_msafluid`. Compile-time consts for N, ITERS, sampling mode,
  solver-Hz target. Periodic `info!` of achieved solver Hz vs target.

### Out of scope

- Any wiring through the `lp-engine` pipeline. This is a standalone test,
  not an engine node.
- Persisting the demo as an example. It lives only in `fw-esp32/src/tests/`.
- Any changes to the existing mono perf test. `test_msafluid` keeps working
  with the new RGB-capable solver by writing dye into the `r` channel only.
- The actual creation of a "texture-level Rust builtin" abstraction in
  lpfx. The decision and design for that lives in a follow-up.

## Current state of the codebase

Relevant pieces that already exist and that this plan builds on:

- **`lp-fw/fw-esp32/src/tests/msafluid_solver.rs`** — mono Stam solver in
  Q32. RGB needs to be added. Knobs (`set_solver_iterations`) already
  parameterised. ~510 LOC.
- **`lp-fw/fw-esp32/src/tests/test_msafluid.rs`** — perf-only test
  harness. Will keep working unchanged once the solver gets RGB (single-
  channel `add_color_at_cell` becomes a 3-arg call passing zeros for g,b).
- **`lp-fw/fw-esp32/src/tests/test_dither.rs`** — reference for how to
  set up `Rmt` + `LedChannel` + `DisplayPipeline` from the test harness.
  Pin is gpio18 there; we use gpio4 here.
- **`lp-core/lp-shared/src/display_pipeline/pipeline.rs`** — triple-
  buffered `DisplayPipeline` with `tick(now_us, out)` and
  `write_frame_8(ts_us, &data)` / `write_frame(ts_us, &expanded_u16)`.
  Lerps prev→current at display Hz when `interpolation_enabled`. This is
  exactly the temporal-interpolation trick we want.
- **`lp-fw/fw-esp32/src/output/`** — `LedChannel::new(rmt, pin, num_leds)`
  abstraction and existing RMT driver. Pin parameter generic over
  `OutputPin`.
- **`lp-fw/fw-esp32/src/board/esp32c6/init.rs`** — `init_board()` returns
  `(sw_int, timg0, rmt_peripheral, usb_device, gpio18, _flash)`. We need
  gpio4 — will need a small adjustment or to thread an extra GPIO out.
- **`examples/basic/src/fixture.fixture/node.json`** — declares the ring
  geometry: center (0.5, 0.5), diameter 1.0, 9 rings with counts
  [1, 8, 12, 16, 24, 32, 40, 48, 60], InnerFirst order, `sample_diameter: 2`.
- **`lp-fw/fw-esp32/src/tests/test_msafluid.rs:setup_pmu_cycle_counter`,
  `read_cycle`** — Espressif PMU CSR helpers (mpcer/mpcmr/mpccr at
  0x7E0/0x7E1/0x7E2). Will be useful for solver-Hz tracking.

Pieces that don't exist yet and the plan adds:

- RGB fields and `add_color_at_cell(i, j, r, g, b)` on `MsaFluidSolver`.
- `emit_circle_with_target` and friends, in their own module.
- `FluidPulser`-style 3-emitter oscillator.
- Fixture-geometry ring-position LUT (241 (x,y) pairs from RingArray
  parameters; computed once at boot).
- Bilinear / nearest grid sampler.
- Test runner that ties it all together: solver loop + emitter ticker +
  per-frame sampling into the `DisplayPipeline` + `tick` to RMT.

## Questions

### Confirmation-style (answered)

| #  | Question                                                                  | Answer                                                                                                              |
|----|---------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------|
| Q1 | Plan dir `docs/plans/2026-04-20-fluid-demo/`?                             | Yes.                                                                                                                |
| Q2 | Add RGB to existing `MsaFluidSolver` (replacing mono) or new struct?      | Replace existing. `test_msafluid` keeps working by writing dye into r only.                                         |
| Q3 | Hardcode `RingArray` geometry in the test, or parse JSON at runtime?      | Hardcode. Test isn't supposed to track engine/JSON evolution.                                                       |
| Q4 | Output pin: gpio4 (basic fixture wiring) or gpio18 (existing test wiring)?| gpio4. Match the real fixture.                                                                                      |
| Q5 | Color readout: lp2014 normalize-by-max, or simple clamp?                  | lp2014 normalize-by-max, to match the reference. Expose as `const NORMALIZE_BY_MAX: bool` so we can flip if needed. |
| Q6 | Brightness: hardcode at 0.12 (basic fixture) or runtime knob?             | Hardcode 0.12.                                                                                                      |

### Discussion-style (answered)

- **Q7. Reuse `DisplayPipeline` for temporal interpolation?** → Reuse if
  possible. It's already in `lp-shared`, no_std, gives us prev/current
  lerp + LUT + dither, and using it doubles as a real test of the
  production pipeline. Pull it in directly; only fall back to a
  standalone lerp if integration turns out to be painful.
- **Q8. Frame timing strategy?** → Target a configurable solver Hz
  (`SOLVER_HZ_TARGET` const). Display ticks at 60Hz via interpolation.
  We specifically want to try low-Hz solver pinning (e.g. 10Hz) and see
  if interpolation makes it look acceptable. Print achieved solver Hz vs
  target periodically.
- **Q9. RGB `add_color_at_cell` signature?** → Three Q32 args
  `(r, g, b)`. Mirrors lp2014. `test_msafluid` will pass `(dye, ZERO,
  ZERO)`.

## Notes

- Hand-LICM and Q32 micro-opts in the existing solver should carry over to
  the RGB path — diffuse_g / diffuse_b are structurally identical to
  diffuse_r.
- `linear_solver_uv` (combined u+v) does NOT need an analogous
  `linear_solver_rgb`; r/g/b diffusion is independent and benefits little
  from fusion (no shared sums).
- We may discover the demo is too slow even at iters=2, N=16 RGB. That's
  OK — the answer "fluid doesn't fit on this MCU" is itself a valid
  outcome and feeds the architecture decision.
- The demo should run forever in a loop; serial logger should print
  achieved Hz once per second.

## Reference snippets from lp2014

- Solver: `~/dev/personal/lightPlayer/PlayerCore/src/main/java/
  com/lightatplay/lightplayer/rendering/msafluid/MSAFluidSolver2D.java`
- FluidRenderer (emit primitives + render-to-image):
  `…/rendering/FluidRenderer.kt`
- FluidPulser (3-emitter oscillator config + behavior):
  `…/program/fluid/FluidPulser.kt`
