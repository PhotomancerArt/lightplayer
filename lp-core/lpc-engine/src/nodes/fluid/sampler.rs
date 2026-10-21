//! Sampling helpers for fluid visual products.

use lps_q32::Q32;

use super::solver::MsaFluidSolver;

/// Sample the current fluid color with nearest-neighbor filtering.
pub fn sample_rgb_nearest_q16(solver: &MsaFluidSolver, x_q16: i32, y_q16: i32) -> (Q32, Q32, Q32) {
    let nx = solver.nx();
    let ny = solver.ny();
    let stride = solver.stride();
    let i = q16_to_cell(x_q16, nx);
    let j = q16_to_cell(y_q16, ny);
    let c = i + j * stride;
    (solver.r()[c], solver.g()[c], solver.b()[c])
}

/// Sample the current fluid color with bilinear filtering.
///
/// Cell `i` (1-based) is read at its **left/top corner**, `(i − 1) / n`,
/// while [`super::stamp_emitter`] puts an emitter's dye in the cell whose
/// span holds it — centred at `(i − 0.5) / n`. So pixels mode reads its dye
/// back half a cell up-and-left of where it stamped it. It is kept exactly
/// as it is: pixels mode is byte-identical to every fluid rendered before
/// pattern space existed. Pattern mode samples with
/// [`sample_rgb_bilinear_centred_q16`], which has no offset.
pub fn sample_rgb_bilinear_q16(solver: &MsaFluidSolver, x_q16: i32, y_q16: i32) -> (Q32, Q32, Q32) {
    let nx = solver.nx();
    let ny = solver.ny();
    let stride = solver.stride();
    let (x0, x1, tx) = q16_to_cell_pair(x_q16, nx);
    let (y0, y1, ty) = q16_to_cell_pair(y_q16, ny);

    let c00 = x0 + y0 * stride;
    let c10 = x1 + y0 * stride;
    let c01 = x0 + y1 * stride;
    let c11 = x1 + y1 * stride;

    (
        bilinear_channel(
            solver.r()[c00],
            solver.r()[c10],
            solver.r()[c01],
            solver.r()[c11],
            tx,
            ty,
        ),
        bilinear_channel(
            solver.g()[c00],
            solver.g()[c10],
            solver.g()[c01],
            solver.g()[c11],
            tx,
            ty,
        ),
        bilinear_channel(
            solver.b()[c00],
            solver.b()[c10],
            solver.b()[c01],
            solver.b()[c11],
            tx,
            ty,
        ),
    )
}

/// Sample the current fluid color with bilinear filtering over **cell
/// centres**: cell `i` (1-based) is read exactly at `(i − 0.5) / n`, the
/// centre of the span its dye is stamped into. Between centres it blends the
/// two neighbours; outside the outermost centres it holds the edge cell
/// (clamp). `x_q16` / `y_q16` are grid uv, 0 → 1 edge to edge.
pub fn sample_rgb_bilinear_centred_q16(
    solver: &MsaFluidSolver,
    x_q16: i32,
    y_q16: i32,
) -> (Q32, Q32, Q32) {
    let nx = solver.nx();
    let ny = solver.ny();
    let stride = solver.stride();
    let (x0, x1, tx) = q16_to_centred_cell_pair(x_q16, nx);
    let (y0, y1, ty) = q16_to_centred_cell_pair(y_q16, ny);

    let c00 = x0 + y0 * stride;
    let c10 = x1 + y0 * stride;
    let c01 = x0 + y1 * stride;
    let c11 = x1 + y1 * stride;

    (
        bilinear_channel(
            solver.r()[c00],
            solver.r()[c10],
            solver.r()[c01],
            solver.r()[c11],
            tx,
            ty,
        ),
        bilinear_channel(
            solver.g()[c00],
            solver.g()[c10],
            solver.g()[c01],
            solver.g()[c11],
            tx,
            ty,
        ),
        bilinear_channel(
            solver.b()[c00],
            solver.b()[c10],
            solver.b()[c01],
            solver.b()[c11],
            tx,
            ty,
        ),
    )
}

/// Sample the current fluid color as RGBA16 unorm with nearest-neighbor filtering.
pub fn sample_rgba16_nearest_q16(solver: &MsaFluidSolver, x_q16: i32, y_q16: i32) -> [u16; 4] {
    let (r, g, b) = sample_rgb_nearest_q16(solver, x_q16, y_q16);
    rgba16_from_rgb(r, g, b)
}

/// Sample the current fluid color as RGBA16 unorm with bilinear filtering.
pub fn sample_rgba16_bilinear_q16(solver: &MsaFluidSolver, x_q16: i32, y_q16: i32) -> [u16; 4] {
    let (r, g, b) = sample_rgb_bilinear_q16(solver, x_q16, y_q16);
    rgba16_from_rgb(r, g, b)
}

/// [`sample_rgb_bilinear_centred_q16`] as RGBA16 unorm.
pub fn sample_rgba16_bilinear_centred_q16(
    solver: &MsaFluidSolver,
    x_q16: i32,
    y_q16: i32,
) -> [u16; 4] {
    let (r, g, b) = sample_rgb_bilinear_centred_q16(solver, x_q16, y_q16);
    rgba16_from_rgb(r, g, b)
}

fn rgba16_from_rgb(r: Q32, g: Q32, b: Q32) -> [u16; 4] {
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

fn q16_to_cell_pair(coord: i32, len: usize) -> (usize, usize, Q32) {
    let clamped = coord.clamp(0, 65535) as i64;
    let scaled = clamped * len as i64;
    let index = (scaled >> 16).clamp(0, len.saturating_sub(1) as i64) as usize;
    let frac = if index + 1 >= len {
        Q32::ZERO
    } else {
        Q32::from_fixed((scaled & 0xFFFF) as i32)
    };
    let cell0 = index + 1;
    let cell1 = (cell0 + 1).min(len);
    (cell0, cell1, frac)
}

/// The two cells around `coord` (grid uv, Q16.16) on a centre lattice —
/// cell `k` (0-based) centred at `(k + 0.5) / len` — and the blend between
/// them.
fn q16_to_centred_cell_pair(coord: i32, len: usize) -> (usize, usize, Q32) {
    let len = len.max(1) as i64;
    // Position in cell units, measured from the first cell's centre.
    let scaled = i64::from(coord.clamp(0, 65536)) * len - 32768;
    let scaled = scaled.clamp(0, (len - 1) << 16);
    let index = (scaled >> 16) as usize;
    let frac = Q32::from_fixed((scaled & 0xFFFF) as i32);
    let cell0 = index + 1;
    let cell1 = (cell0 + 1).min(len as usize);
    (cell0, cell1, frac)
}

fn bilinear_channel(c00: Q32, c10: Q32, c01: Q32, c11: Q32, tx: Q32, ty: Q32) -> Q32 {
    c00.mix(c10, tx).mix(c01.mix(c11, tx), ty)
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

    #[test]
    fn bilinear_sampling_blends_neighbor_cells() {
        let mut solver = MsaFluidSolver::new(2, 2);
        solver.add_color_at_cell(1, 1, Q32::ZERO, Q32::ZERO, Q32::ZERO);
        solver.add_color_at_cell(2, 1, Q32::ONE, Q32::ZERO, Q32::ZERO);
        solver.add_color_at_cell(1, 2, Q32::ZERO, Q32::ONE, Q32::ZERO);
        solver.add_color_at_cell(2, 2, Q32::ZERO, Q32::ZERO, Q32::ONE);
        solver.update();

        let (r, g, b) = sample_rgb_bilinear_q16(&solver, 16384, 16384);

        assert!(r > Q32::ZERO && r < Q32::ONE);
        assert!(g > Q32::ZERO && g < Q32::ONE);
        assert!(b > Q32::ZERO && b < Q32::ONE);
    }
}
