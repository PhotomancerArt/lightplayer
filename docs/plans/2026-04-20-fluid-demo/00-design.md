# 2026-04-20 Fluid Demo — Design

See `00-notes.md` for scope, current state, and answered questions.

## File structure

```
lp-fw/fw-esp32/
├── Cargo.toml                                 # UPDATE: add `test_fluid_demo` feature
├── src/
│   ├── main.rs                                # UPDATE: dispatch test_fluid_demo
│   ├── board/esp32c6/
│   │   └── init.rs                            # UPDATE: add gpio4 to init_board return tuple
│   ├── output/mod.rs                          # UPDATE: gate provider on test_fluid_demo too
│   ├── serial/...                             # UPDATE: gate test-only sites for new feature
│   └── tests/
│       ├── mod.rs                             # UPDATE: register fluid_demo module
│       ├── msafluid_solver.rs                 # UPDATE: add g, b channels (RGB)
│       ├── test_msafluid.rs                   # UPDATE: 3-arg add_color_at_cell
│       ├── test_dither.rs                     # UPDATE: destructure new gpio4 from init_board
│       ├── test_rmt.rs                        # UPDATE: destructure new gpio4 from init_board
│       ├── test_gpio.rs                       # UPDATE: destructure new gpio4 from init_board
│       ├── test_usb.rs                        # UPDATE: destructure new gpio4 from init_board
│       ├── test_json.rs                       # UPDATE: destructure new gpio4 from init_board
│       └── fluid_demo/                        # NEW: demo lives in its own dir
│           ├── mod.rs                         # NEW: pub use entry point
│           ├── runner.rs                      # NEW: run_fluid_demo (test entry point)
│           ├── emitters.rs                    # NEW: emit_circle_*, FluidPulser
│           ├── ring_geometry.rs               # NEW: 241 (x,y) lamp positions LUT
│           ├── sampler.rs                     # NEW: nearest/bilinear grid sampler
│           └── readout.rs                     # NEW: (r,g,b) Q32 → u8 RGB per pixel

justfile                                       # UPDATE: fwtest-fluid-demo-esp32c6 recipe

docs/plans/2026-04-20-fluid-demo/
├── 00-notes.md                                # done
├── 00-design.md                               # this file
├── 01-rgb-solver.md                           # phase 1
├── 02-board-gpio4.md                          # phase 2
├── 03-emitters-and-pulser.md                  # phase 3
├── 04-geometry-sampler-readout.md             # phase 4
├── 05-runner-wiring.md                        # phase 5
├── 06-cleanup-and-validate.md                 # phase 6
└── summary.md                                 # at end
```

## Conceptual architecture

```
                 ┌────────────────────────────────────────────────┐
                 │  fluid_demo::runner (only hw-aware module)     │
                 │                                                │
                 │  loop {                                        │
                 │    now_us = Instant::now().as_micros()         │
                 │                                                │
                 │    if (now_us - last_solver_us) >= solver_p_us:│
                 │      pulser.tick(now_ms)  ──┐                  │
                 │      solver.update()        │ emits via        │
                 │                             ▼ add_color/force  │
                 │      readout::frame(solver, geom, &mut rgb_buf)│
                 │      pipeline.write_frame_from_u8(now_us,      │
                 │                                  &rgb_buf)     │
                 │      solver_count += 1                         │
                 │      last_solver_us = now_us                   │
                 │                                                │
                 │    pipeline.tick(now_us, &mut led_buf)         │
                 │    led_channel = led_channel                   │
                 │      .start_transmission(&led_buf)             │
                 │      .wait_complete()                          │
                 │    display_count += 1                          │
                 │                                                │
                 │    if (now_us - last_log_us) >= 1_000_000:     │
                 │      info!("solver={solver_hz} display={d_hz}")│
                 │      reset counters                            │
                 │  }                                             │
                 └────────────────────────────────────────────────┘
                       │            │            │            │
                       ▼            ▼            ▼            ▼
        ┌──────────────────┐ ┌──────────┐ ┌─────────┐ ┌──────────────┐
        │ MsaFluidSolver   │ │FluidPulse│ │ring_geom│ │DisplayPipeline│
        │ (RGB)            │ │           │ │         │ │ (lp-shared,  │
        │                  │ │ tick:     │ │ const   │ │  reused)     │
        │ u, v             │ │  3 emit   │ │ LAMPS=  │ │              │
        │ r, g, b          │ │  circles  │ │   241   │ │ write_frame_ │
        │ + _old buffers   │ │  with     │ │ const   │ │  from_u8     │
        │                  │ │  oscillat-│ │ POS:    │ │ tick (lerp + │
        │ add_color(r,g,b) │ │  ing pos, │ │  [Pt;   │ │  LUT+dither) │
        │ add_force(vx,vy) │ │  hue, vel │ │   241]  │ └──────────────┘
        │ update():        │ │           │ │         │        │
        │  diffuse_uv      │ └───────────┘ └─────────┘        ▼
        │  diffuse_r/g/b   │                            ┌────────────┐
        │  advect_*        │            ┌──── sampler:  │ LedChannel │
        │  project         │ ◄──────────┤ nearest|      │  RMT, gpio4│
        │  fade_r/g/b      │            │  bilinear     │  241 leds  │
        └──────────────────┘            │ (returns Q32) └────────────┘
                                        └────── readout::frame:
                                                normalize-by-max → u8
```

## Key design points

### 1. Only `runner.rs` is hardware-aware

Every other module in `fluid_demo/` is pure no_std compute that takes data
in and returns data out. This keeps the knob-twiddling (`const`s in
`runner.rs`) cleanly separated from the algorithms, and makes each piece
trivially testable in isolation if we ever want to.

### 2. RGB extension of the solver, in place

The existing `MsaFluidSolver` becomes RGB. `add_color_at_cell` takes
`(r, g, b)` Q32 args. `test_msafluid` (the perf test) keeps working by
passing `(dye, ZERO, ZERO)` — its measurements remain comparable to the
baseline data because the b/g channels carry zero work after a few frames
of fade. (We're characterising the solver, not the test pattern.)

`u`/`v` velocity solve is shared across channels (same as lp2014).
`diffuse_r` / `diffuse_g` / `diffuse_b` each call `linear_solver`
independently. `advect_g` / `advect_b` mirror `advect_r`. Same for
`fade_g` / `fade_b` and `swap_g` / `swap_b`.

### 3. Emitters: 3-particle FluidPulser pattern, in Q32 at the boundary

Per-frame emitter math (sin/cos for position, hue→rgb, etc.) runs 3 times
per solver step, not in the inner loop. f32 + `micromath` is fine here;
convert to Q32 only at the call boundary into `solver.add_color_at_cell`
/ `add_force_at_cell`. Keeps the port readable and matches lp2014 numerics
without hand-tuning a Q32 sin table.

`emit_circle_with_angle` stamps a colored disk into the solver:

```text
for x in [-radius, radius] step 1/N {
  for y in [-radius, radius] step 1/N {
    if x*x + y*y <= radius*radius {
      add_color_at_cell(grid_pos_of(fx + x, fy + y), r, g, b)
      add_force_at_cell(grid_pos_of(fx + x, fy + y), vx, vy)
    }
  }
}
```

`emit_circle_with_target(fx, fy, tx, ty, ...)` computes
`atan2(ty-fy, tx-fx)` and delegates to `emit_circle_with_angle`. Direct
port of `FluidRenderer.kt`.

### 4. Ring geometry: hardcoded 241-lamp LUT

`ring_geometry.rs`:

```rust
pub const RING_LAMP_COUNTS: [u8; 9] = [1, 8, 12, 16, 24, 32, 40, 48, 60];
pub const NUM_LAMPS: usize = 241;

pub struct LampPosition { pub x: Q32, pub y: Q32 } // in [0, 1]^2

/// Build positions once at boot, returned in InnerFirst order.
pub fn build_ring_positions() -> [LampPosition; NUM_LAMPS] { ... }
```

Position formula matches the lp-engine `RingArray` mapping:

```text
center = (0.5, 0.5)
max_radius = 0.5  (diameter 1.0)

for ring in 0..9 {
  radius = if ring == 0 { 0 } else { (ring as f32) / 8.0 * max_radius }
  N = RING_LAMP_COUNTS[ring]
  for k in 0..N {
    angle = (k as f32) / (N as f32) * 2π + offset_angle  // 0
    x = 0.5 + radius * cos(angle)
    y = 0.5 + radius * sin(angle)
  }
}
```

Built at boot using f32; stored as Q32 for the sampler.

### 5. Sampling: nearest and bilinear, behind a `const` knob

Position is in `[0, 1]²`. Solver has a 1-cell border, so usable interior is
`[1, nx]` in grid units. Mapping:

```text
gx = 1 + x * nx         // floats; for nearest, round
gy = 1 + y * ny

nearest:  field[idx(round(gx), round(gy), stride)]
bilinear: lerp4(field, gx, gy)
```

Both return `Q32`. Sampler trait is the const knob; readout picks at
compile time.

### 6. Readout: per-lamp normalize-by-max, into a `[u8; 723]`

Mirror lp2014:

```rust
let fr = sample_r * intensity;
let fg = sample_g * intensity;
let fb = sample_b * intensity;
let max = max(Q32::ONE, fr, fg, fb);
let r_u8 = clamp_u8(fr / max);
let g_u8 = clamp_u8(fg / max);
let b_u8 = clamp_u8(fb / max);
```

`NORMALIZE_BY_MAX: bool` const; if false, fall back to plain `clamp_u8(fc * 255)`.

### 7. Display pipeline: reuse `lp_shared::DisplayPipeline`

- Construct with `DisplayPipelineOptions { brightness: 0.12,
  interpolation_enabled: true, dithering_enabled: true, lut_enabled: true,
  lum_power: 2.0, white_point: [1.0, 1.0, 1.0] }`.
- `pipeline.write_frame_from_u8(now_us, &rgb_buf)` once per solver step.
- `pipeline.tick(now_us, &mut led_buf)` every loop iteration.

This gives us temporal interpolation between solver frames for free, plus
LUT and dither — the full production output stack.

### 8. Loop timing

Compile-time:

```rust
const GRID_N: usize = 32;            // square grid
const SOLVER_ITERS: usize = 4;       // jacobi iter count
const SOLVER_HZ_TARGET: u32 = 15;    // tunable: 5, 10, 15, 20, …
const SAMPLE_MODE: SampleMode = SampleMode::Bilinear;
const NORMALIZE_BY_MAX: bool = true;
const INTENSITY: f32 = 2.5;          // matches lp2014 default
```

Runtime: solver stepped only when `now_us - last_solver_us >= solver_period_us`.
Display ticks are paced by RMT send time (~7ms for 241 leds); the loop
busy-spins between solver steps doing display ticks.

### 9. Hz logging

Counters of solver steps and display ticks, logged once per second via
`log::info!`. Lets us see immediately whether solver is keeping up with
target and what display rate we're achieving.

### 10. Board change: gpio4 added to init_board tuple

`init_board()` returns one extra peripheral: `GPIO4`. All existing test
destructures (`test_dither`, `test_rmt`, `test_gpio`, `test_usb`,
`test_json`, `test_msafluid`) get a new `_gpio4` element. Mechanical.

## Phase outline (for phase file writer)

1. **RGB extension of MsaFluidSolver** — sub-agent, parallel: 2.
   Add `g`/`g_old`/`b`/`b_old` fields, `add_color_at_cell(i, j, r, g, b)`,
   `diffuse_g`/`diffuse_b`/`swap_g`/`swap_b`/`advect_g`/`advect_b`/
   `fade_g`/`fade_b`. Update `update()`. Update `test_msafluid.rs`
   `add_color_at_cell` callsites to pass `(dye, ZERO, ZERO)`.

2. **Expose gpio4 from init_board** — sub-agent, parallel: 1.
   Add to return tuple. Update all destructures in `tests/test_*.rs` and
   any other call sites.

3. **Emitters + FluidPulser port** — sub-agent, parallel: 4, after 1.
   `fluid_demo/emitters.rs` with `emit_directional`,
   `emit_circle_with_angle`, `emit_circle_with_target`, plus a
   `FluidPulser` struct with `tick(time_ms)` + `Config` defaults from
   `FluidPulser.kt`. f32 inside; Q32 at boundary into solver.

4. **Ring geometry + sampler + readout** — sub-agent, parallel: 3, after 1.
   Three small sibling files in `fluid_demo/`. `ring_geometry.rs` builds
   the `[LampPosition; 241]` LUT. `sampler.rs` provides `sample_nearest`
   and `sample_bilinear`. `readout.rs` provides `frame(solver, geom, mode,
   normalize_by_max, intensity, &mut [u8; 723])`.

5. **Runner + Cargo feature + main.rs dispatch + justfile** — sub-agent
   (supervised), after 1–4.
   `fluid_demo/runner.rs` ties it all together. New `test_fluid_demo`
   feature in `Cargo.toml`. `main.rs` dispatch matching the
   `test_msafluid` pattern. Gate other test-only modules to also disable
   when `test_fluid_demo` is set. New `justfile` recipe
   `fwtest-fluid-demo-esp32c6`.

6. **Cleanup, validate, summary** — sub-agent (supervised).
   Lint-clean for both `esp32c6` and `esp32c6,test_fluid_demo` feature
   combos. Grep diff for `TODO`/`dbg!`/commented-out code. Write
   `summary.md`.
