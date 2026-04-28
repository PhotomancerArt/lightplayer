//! 3-emitter `FluidPulser` port from lp2014 + emit primitives from
//! `FluidRenderer.kt`. f32 inside (not a hot loop — 3 calls per solver
//! step), Q32 at the boundary into the solver.

use libm::{atan2f, cosf, floorf, sinf};
use lps_q32::Q32;

use crate::tests::msafluid_solver::MsaFluidSolver;

/// Stateful pulser that emits 3 colored circles per solver step in
/// slowly-oscillating patterns toward `target`.
pub struct FluidPulser {
    pub config: PulserConfig,
}

/// Defaults from `FluidPulser.kt`.
#[expect(
    dead_code,
    reason = "Parity with lp2014 config; angle_period_ms / use_palette not wired in tick yet (see TODO on use_palette)."
)]
pub struct PulserConfig {
    pub x_period_ms: f32,     // 7000.0
    pub y_period_ms: f32,     // 13000.0
    pub size_period_ms: f32,  // 5000.0
    pub angle_period_ms: f32, // 37000.0  (kept for parity; unused by tick)
    pub hue_period_ms: f32,   // 23000.0
    // TODO: `tick` uses full-saturation HSV only; lp2014 branches on palette via `colorFor` when true.
    pub use_palette: bool, // false
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
                solver, fx, fy, target_x, target_y, radius, r, g, b, velocity, 0.7,
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
    emit_circle_with_angle(solver, fx, fy, radius, angle, r, g, b, velocity, intensity);
}

/// Stamp a colored disk into the solver, pushing velocity along
/// `angle_radians`. Direct port of `FluidRenderer.emitCircleWithAngle`.
#[allow(
    clippy::too_many_arguments,
    reason = "direct port; matches lp2014 API shape"
)]
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
#[allow(
    clippy::too_many_arguments,
    reason = "direct port; matches lp2014 API shape"
)]
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
    normal_cos_range(t, p, 0.04, 0.08)
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
    let h = (hue - floorf(hue)) * 6.0;
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
