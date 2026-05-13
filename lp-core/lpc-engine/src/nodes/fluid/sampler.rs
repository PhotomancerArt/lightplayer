//! Sampling helpers for fluid visual products.

use lps_q32::Q32;

use super::solver::MsaFluidSolver;

/// Sample the current fluid color with nearest-neighbor interpolation.
pub fn sample_rgb_nearest_q16(solver: &MsaFluidSolver, x_q16: i32, y_q16: i32) -> (Q32, Q32, Q32) {
    let nx = solver.nx();
    let ny = solver.ny();
    let stride = solver.stride();
    let i = q16_to_cell(x_q16, nx);
    let j = q16_to_cell(y_q16, ny);
    let c = i + j * stride;
    (solver.r()[c], solver.g()[c], solver.b()[c])
}

/// Sample the current fluid color as RGBA16 unorm.
pub fn sample_rgba16_nearest_q16(solver: &MsaFluidSolver, x_q16: i32, y_q16: i32) -> [u16; 4] {
    let (r, g, b) = sample_rgb_nearest_q16(solver, x_q16, y_q16);
    [
        r.to_u16_saturating(),
        g.to_u16_saturating(),
        b.to_u16_saturating(),
        u16::MAX,
    ]
}

fn q16_to_cell(coord: i32, len: usize) -> usize {
    let clamped = coord.clamp(0, 65535) as i64;
    let index = ((clamped * len as i64) >> 16).clamp(0, len.saturating_sub(1) as i64);
    index as usize + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_nearest_reads_nonzero_cell() {
        let mut solver = MsaFluidSolver::new(4, 4);
        solver.add_color_at_cell(3, 3, Q32::ONE, Q32::ZERO, Q32::ZERO);
        solver.update();

        let rgba = sample_rgba16_nearest_q16(&solver, 32768, 32768);

        assert!(rgba[0] > 0);
        assert_eq!(rgba[3], u16::MAX);
    }
}
