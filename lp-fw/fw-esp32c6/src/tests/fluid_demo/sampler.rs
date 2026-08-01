//! Sample the fluid solver's r/g/b fields at arbitrary normalized
//! (x, y) ∈ [0, 1]². Compile-time switch between nearest-neighbor and
//! bilinear interpolation.
//!
//! The solver grid has interior dimensions `nx × ny` with a 1-cell
//! ghost border; cell `(i, j)` for `1 ≤ i ≤ nx, 1 ≤ j ≤ ny` is the
//! interior. We map (x, y) into the interior. Edge cells get
//! nearest-neighbor sampled regardless of `SAMPLER_BILINEAR` to avoid
//! reaching into the ghost border.

use libm::floorf;
use lps_q32::Q32;

use crate::tests::msafluid_solver::MsaFluidSolver;

/// Compile-time choice of interpolation. `false` = nearest, `true` =
/// bilinear. Bilinear costs ~4× the loads but is much smoother for the
/// circular fixture; nearest is the diagnostic baseline.
pub const SAMPLER_BILINEAR: bool = true;

/// Sample (r, g, b) from the solver at normalized (x, y) ∈ [0, 1]².
pub fn sample_rgb(solver: &MsaFluidSolver, x: f32, y: f32) -> (Q32, Q32, Q32) {
    if SAMPLER_BILINEAR {
        sample_rgb_bilinear(solver, x, y)
    } else {
        sample_rgb_nearest(solver, x, y)
    }
}

/// Nearest-neighbor sample.
pub fn sample_rgb_nearest(solver: &MsaFluidSolver, x: f32, y: f32) -> (Q32, Q32, Q32) {
    let nx = solver.nx();
    let ny = solver.ny();
    let stride = solver.stride();
    let i = ((x * nx as f32) as i32).clamp(0, nx as i32 - 1) as usize + 1;
    let j = ((y * ny as f32) as i32).clamp(0, ny as i32 - 1) as usize + 1;
    let c = i + j * stride;
    (solver.r()[c], solver.g()[c], solver.b()[c])
}

/// Bilinear sample. Falls back to nearest at the very edge to avoid
/// reading outside the interior.
pub fn sample_rgb_bilinear(solver: &MsaFluidSolver, x: f32, y: f32) -> (Q32, Q32, Q32) {
    let nx = solver.nx();
    let ny = solver.ny();
    let stride = solver.stride();

    // Continuous cell-center coordinates in interior space (1..=nx).
    let fx = x * nx as f32 + 0.5;
    let fy = y * ny as f32 + 0.5;

    let i0 = (floorf(fx) as i32).clamp(1, nx as i32 - 1) as usize;
    let j0 = (floorf(fy) as i32).clamp(1, ny as i32 - 1) as usize;
    let i1 = i0 + 1;
    let j1 = j0 + 1;

    let tx_f = (fx - i0 as f32).clamp(0.0, 1.0);
    let ty_f = (fy - j0 as f32).clamp(0.0, 1.0);
    let tx = Q32::from_f32_wrapping(tx_f);
    let ty = Q32::from_f32_wrapping(ty_f);

    let c00 = i0 + j0 * stride;
    let c10 = i1 + j0 * stride;
    let c01 = i0 + j1 * stride;
    let c11 = i1 + j1 * stride;

    let lerp = |a: Q32, b: Q32, t: Q32| a + (b - a) * t;

    let r = lerp(
        lerp(solver.r()[c00], solver.r()[c10], tx),
        lerp(solver.r()[c01], solver.r()[c11], tx),
        ty,
    );
    let g = lerp(
        lerp(solver.g()[c00], solver.g()[c10], tx),
        lerp(solver.g()[c01], solver.g()[c11], tx),
        ty,
    );
    let b = lerp(
        lerp(solver.b()[c00], solver.b()[c10], tx),
        lerp(solver.b()[c01], solver.b()[c11], tx),
        ty,
    );
    (r, g, b)
}
