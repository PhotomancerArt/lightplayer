# Phase 03 — Emitters and FluidPulser port

**Tags:** sub-agent: yes, parallel: 4, depends on phase 1.

## Scope of phase

Create the emitter primitives and the `FluidPulser` 3-emitter
oscillator pattern in `lp-fw/fw-esp32/src/tests/fluid_demo/`. Port from
the lp2014 references:

- `~/dev/personal/lightPlayer/PlayerCore/src/main/java/com/lightatplay/lightplayer/rendering/FluidRenderer.kt`
  (`emitDirectional`, `emitCircleWithAngle`, `emitCircleWithTarget`)
- `~/dev/personal/lightPlayer/PlayerCore/src/main/java/com/lightatplay/lightplayer/program/fluid/FluidPulser.kt`
  (3-emitter time-oscillator config + render loop)

Add `lp-fw/fw-esp32/src/tests/fluid_demo/mod.rs` with `pub mod
emitters;`.

### Out of scope

- `runner.rs` (phase 5), `ring_geometry.rs`, `sampler.rs`,
  `readout.rs` (phase 4).
- Any registration of `fluid_demo` in `tests/mod.rs` — phase 5
  registers the module.
- Any change to `MsaFluidSolver`, beyond consuming the RGB API added in
  phase 1.
- Adding any new Cargo dependency. `libm` is already in the workspace
  and accessible transitively via `lps-q32`; **either reuse that** or, if
  it's not in scope for this crate, add `libm = "0.2"` to
  `lp-fw/fw-esp32/Cargo.toml` (in scope for this phase). Do not add any
  other dep.

## Code organization reminders

- Granular file structure, one concept per file.
- Place abstract things, entry points, and tests near the **top** of
  each file.
- Place helper utility functions at the **bottom** of each file.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment so it can be found
  later.

## Sub-agent reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope. Stay strictly within "Scope of phase".
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix
  them.
- Do **not** disable, skip, or weaken existing tests to make the build
  pass.
- If something blocks completion (ambiguity, unexpected design issue),
  stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations
  from this phase plan.

## Implementation details

### File layout

Create:

```
lp-fw/fw-esp32/src/tests/fluid_demo/
├── mod.rs            # pub mod emitters;
└── emitters.rs       # everything below
```

The `mod.rs` is one line:

```rust
pub mod emitters;
```

### `emitters.rs` skeleton

Top of file, then helpers at the bottom:

```rust
//! 3-emitter `FluidPulser` port from lp2014 + emit primitives from
//! `FluidRenderer.kt`. f32 inside (not a hot loop — 3 calls per solver
//! step), Q32 at the boundary into the solver.

use libm::{atan2f, cosf, sinf};
use lps_q32::Q32;

use crate::tests::msafluid_solver::MsaFluidSolver;

/// Stateful pulser that emits 3 colored circles per solver step in
/// slowly-oscillating patterns toward `target`.
pub struct FluidPulser {
    pub config: PulserConfig,
}

/// Defaults from `FluidPulser.kt`.
pub struct PulserConfig {
    pub x_period_ms: f32,     // 7000.0
    pub y_period_ms: f32,     // 13000.0
    pub size_period_ms: f32,  // 5000.0
    pub angle_period_ms: f32, // 37000.0  (kept for parity; unused by tick)
    pub hue_period_ms: f32,   // 23000.0
    pub use_palette: bool,    // false
}

impl Default for PulserConfig {
    fn default() -> Self {
        Self {
            x_period_ms: 7000.0,
            y_period_ms: 13000.0,
            size_period_ms: 5000.0,
            angle_period_ms: 37000.0,
            hue_period_ms: 23000.0,
            use_palette: false,
        }
    }
}

impl FluidPulser {
    /// Step all 3 emitters into the solver. Mirrors
    /// `FluidPulser.internalRender` in lp2014.
    pub fn tick(
        &mut self,
        solver: &mut MsaFluidSolver,
        time_ms: u64,
        target_x: f32,
        target_y: f32,
        intensity: f32,
    ) {
        for i in 0..3 {
            // Per-emitter time scaling: 1.0, 1.2, 1.44.
            let scale = match i {
                0 => 1.0,
                1 => 1.2,
                _ => 1.44,
            };
            let time = ((time_ms as f64) * scale) as u64;

            let fx = x_pos(time, &self.config);
            let fy = y_pos(time, &self.config);
            let radius = size(time, &self.config) * intensity;
            let velocity = velocity_osc(time) / 30.0;
            let h = hue(time, &self.config);
            let (r, g, b) = hsv_to_rgb_full_sat_val(h);

            emit_circle_with_target(
                solver, fx, fy, target_x, target_y, radius, r, g, b,
                velocity, 0.7,
            );
        }
    }
}

/// Stamp a colored disk into the solver, pushing velocity toward
/// `(target_x, target_y)`. Direct port of
/// `FluidRenderer.emitCircleWithTarget`.
pub fn emit_circle_with_target(
    solver: &mut MsaFluidSolver,
    fx: f32,
    fy: f32,
    target_x: f32,
    target_y: f32,
    radius: f32,
    r: f32,
    g: f32,
    b: f32,
    velocity: f32,
    intensity: f32,
) {
    let angle = atan2f(target_y - fy, target_x - fx);
    emit_circle_with_angle(
        solver, fx, fy, radius, angle, r, g, b, velocity, intensity,
    );
}

/// Stamp a colored disk into the solver, pushing velocity along
/// `angle_radians`. Direct port of `FluidRenderer.emitCircleWithAngle`.
#[allow(clippy::too_many_arguments, reason = "direct port; matches lp2014 API shape")]
pub fn emit_circle_with_angle(
    solver: &mut MsaFluidSolver,
    fx: f32,
    fy: f32,
    radius: f32,
    angle_radians: f32,
    r: f32,
    g: f32,
    b: f32,
    velocity: f32,
    intensity: f32,
) {
    let nx = solver.nx() as f32;
    let ny = solver.ny() as f32;
    let x_step = 1.0 / nx;
    let y_step = 1.0 / ny;
    let r2 = radius * radius;

    let mut dx = -radius;
    while dx <= radius {
        let mut dy = -radius;
        while dy <= radius {
            if dx * dx + dy * dy <= r2 {
                emit_directional(
                    solver,
                    fx + dx,
                    fy + dy,
                    angle_radians,
                    r,
                    g,
                    b,
                    velocity,
                    intensity,
                );
            }
            dy += y_step;
        }
        dx += x_step;
    }
}

/// Single-cell color injection + directional force. Direct port of
/// `FluidRenderer.emitDirectional`.
#[allow(clippy::too_many_arguments, reason = "direct port; matches lp2014 API shape")]
pub fn emit_directional(
    solver: &mut MsaFluidSolver,
    fx: f32,
    fy: f32,
    angle_radians: f32,
    r: f32,
    g: f32,
    b: f32,
    velocity: f32,
    intensity: f32,
) {
    if let Some((i, j)) = grid_cell_for(solver, fx, fy) {
        let cx = cosf(angle_radians);
        let sx = sinf(angle_radians);
        solver.add_color_at_cell(
            i,
            j,
            Q32::from_f32_wrapping(r * intensity),
            Q32::from_f32_wrapping(g * intensity),
            Q32::from_f32_wrapping(b * intensity),
        );
        solver.add_force_at_cell(
            i,
            j,
            Q32::from_f32_wrapping(cx * velocity),
            Q32::from_f32_wrapping(sx * velocity),
        );
    }
}

// ----- helpers (private) ---------------------------------------------

/// Map normalized (x, y) ∈ [0, 1]² to a solver interior cell (1..=nx,
/// 1..=ny). Returns `None` if the position falls outside the grid
/// interior.
fn grid_cell_for(solver: &MsaFluidSolver, x: f32, y: f32) -> Option<(usize, usize)> {
    let nx = solver.nx();
    let ny = solver.ny();
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    let i = (x * nx as f32) as usize + 1;
    let j = (y * ny as f32) as usize + 1;
    let i = i.min(nx);
    let j = j.min(ny);
    Some((i, j))
}

/// `lp2014 LightMath.normalCos(t, period)` — `(1 - cos(t/period * 2π)) * 0.5`.
fn normal_cos(t_in_period: f32, period: f32) -> f32 {
    use core::f32::consts::TAU;
    (1.0 - cosf(t_in_period / period * TAU)) * 0.5
}

/// 4-arg form: `lp2014 LightMath.normalCos(t, period, low, high)`.
fn normal_cos_range(t_in_period: f32, period: f32, low: f32, high: f32) -> f32 {
    normal_cos(t_in_period, period) * (high - low) + low
}

/// FluidPulser xPos: `0.1 + normalCos(t % xPeriod, xPeriod) * 0.8`.
fn x_pos(time_ms: u64, config: &PulserConfig) -> f32 {
    let p = config.x_period_ms;
    let t = (time_ms as f32) % p;
    0.1 + normal_cos(t, p) * 0.8
}

/// FluidPulser yPos: `0.1 + (sin(t % yPeriod / yPeriod * 2π) * 0.5 + 0.5) * 0.8`.
fn y_pos(time_ms: u64, config: &PulserConfig) -> f32 {
    use core::f32::consts::TAU;
    let p = config.y_period_ms;
    let t = (time_ms as f32) % p;
    0.1 + (sinf(t / p * TAU) * 0.5 + 0.5) * 0.8
}

/// FluidPulser size: `normalCos(t % sizePeriod, sizePeriod, 0.06, 0.12)`.
fn size(time_ms: u64, config: &PulserConfig) -> f32 {
    let p = config.size_period_ms;
    let t = (time_ms as f32) % p;
    normal_cos_range(t, p, 0.06, 0.12)
}

/// FluidPulser hue: `normalCos(t % huePeriod, huePeriod, 0.0, 1.0)`.
fn hue(time_ms: u64, config: &PulserConfig) -> f32 {
    let p = config.hue_period_ms;
    let t = (time_ms as f32) % p;
    normal_cos_range(t, p, 0.0, 1.0)
}

/// FluidPulser velocity: `normalCos(t % 12000, 12000, 0.001, 0.01)`.
fn velocity_osc(time_ms: u64) -> f32 {
    let p = 12000.0;
    let t = (time_ms as f32) % p;
    normal_cos_range(t, p, 0.001, 0.01)
}

/// Saturated HSV → RGB with sat = val = 1. Standard 6-sector hue map.
fn hsv_to_rgb_full_sat_val(hue: f32) -> (f32, f32, f32) {
    let h = (hue.rem_euclid(1.0)) * 6.0;
    let i = h as u32;
    let f = h - i as f32;
    let q = 1.0 - f;
    match i % 6 {
        0 => (1.0, f, 0.0),
        1 => (q, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, q, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, q),
    }
}
```

### Notes

- `add_color_at_cell` and `add_force_at_cell` come from phase 1 (RGB
  signature). If they don't exist with the (i, j, r, g, b) signature,
  phase 1 has not landed yet — stop and report.
- `solver.nx()` and `solver.ny()` accessors are added in phase 1. If
  missing, stop and report.
- The module is unreferenced after this phase. The build will succeed
  only if `mod.rs` is not registered anywhere — and it isn't. Phase 5
  wires it up. **Validation here only confirms the existing build still
  works; the new code is compiled as part of phase 5.**

## Validate

From `lp-fw/fw-esp32/`:

```sh
cargo clippy --features esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_msafluid,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

Both must pass clean. The new `emitters.rs` is unreachable from the
crate root in this phase, so it isn't compiled — that's expected and
intentional. Phase 5 will exercise the build of these files.
