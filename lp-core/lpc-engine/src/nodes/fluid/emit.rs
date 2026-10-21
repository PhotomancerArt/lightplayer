//! Fluid emitter stamping, in the Fluid's two frames: pixels mode
//! ([`stamp_emitter`], emitters normalized 0…1 over the grid, y down) and
//! pattern mode ([`stamp_emitter_pattern`], emitters in pattern units, y up,
//! placed through the grid's fit over the lamp box).

use lpc_model::FluidEmitter;
use lps_q32::Q32;

use super::pattern_grid_fit::PatternGridFit;
use super::solver::MsaFluidSolver;

/// Stamp one emitter into the solver's source buffers (pixels mode).
///
/// The emitter position and radius are normalized over the fluid grid. Color
/// and force are added before the next solver update consumes source buffers.
pub fn stamp_emitter(solver: &mut MsaFluidSolver, emitter: &FluidEmitter) {
    let radius = emitter.radius.max(0.0);
    if radius <= 0.0 || emitter.intensity <= 0.0 {
        return;
    }

    let dir_len2 = emitter.dir[0] * emitter.dir[0] + emitter.dir[1] * emitter.dir[1];
    let inv_dir_len = if dir_len2 > 0.000001 {
        1.0 / libm::sqrtf(dir_len2)
    } else {
        0.0
    };
    let vx = emitter.dir[0] * inv_dir_len * emitter.velocity * emitter.intensity;
    let vy = emitter.dir[1] * inv_dir_len * emitter.velocity * emitter.intensity;

    let nx = solver.nx() as f32;
    let ny = solver.ny() as f32;
    let x_step = 1.0 / nx;
    let y_step = 1.0 / ny;
    let r2 = radius * radius;

    let mut dx = -radius;
    while dx <= radius {
        let mut dy = -radius;
        while dy <= radius {
            if dx * dx + dy * dy <= r2
                && let Some((i, j)) =
                    grid_cell_for(solver, emitter.pos[0] + dx, emitter.pos[1] + dy)
            {
                solver.add_color_at_cell(
                    i,
                    j,
                    Q32::from_f32_wrapping(emitter.color[0] * emitter.intensity),
                    Q32::from_f32_wrapping(emitter.color[1] * emitter.intensity),
                    Q32::from_f32_wrapping(emitter.color[2] * emitter.intensity),
                );
                if inv_dir_len > 0.0 && emitter.velocity != 0.0 {
                    solver.add_force_at_cell(
                        i,
                        j,
                        Q32::from_f32_wrapping(vx),
                        Q32::from_f32_wrapping(vy),
                    );
                }
            }
            dy += y_step;
        }
        dx += x_step;
    }
}

/// Stamp one emitter read in **pattern space** (`"coords": "pattern"`).
///
/// `pos` is a pattern-space point (y up), `radius` a pattern-space distance,
/// and the push — `dir` (y up) normalized × `velocity` × `intensity` — a
/// pattern-space vector: pattern units per solver step. `fit` places them on
/// the grid, so the stamp is round on the lamps whatever the grid's or the
/// box's aspect: every cell whose centre lies inside the ellipse gets the
/// dye, and the cell holding `pos` always does, however small the radius.
pub fn stamp_emitter_pattern(
    solver: &mut MsaFluidSolver,
    emitter: &FluidEmitter,
    fit: &PatternGridFit,
) {
    let radius = emitter.radius.max(0.0);
    if radius <= 0.0 || emitter.intensity <= 0.0 {
        return;
    }
    let dir_len2 = emitter.dir[0] * emitter.dir[0] + emitter.dir[1] * emitter.dir[1];
    let inv_dir_len = if dir_len2 > 0.000001 {
        1.0 / libm::sqrtf(dir_len2)
    } else {
        0.0
    };
    let push = emitter.velocity * emitter.intensity * inv_dir_len;
    let [vx, vy] = fit.vector_to_uv([emitter.dir[0] * push, emitter.dir[1] * push]);
    let with_force = inv_dir_len > 0.0 && emitter.velocity != 0.0;
    let color = [
        Q32::from_f32_wrapping(emitter.color[0] * emitter.intensity),
        Q32::from_f32_wrapping(emitter.color[1] * emitter.intensity),
        Q32::from_f32_wrapping(emitter.color[2] * emitter.intensity),
    ];
    let (nx, ny) = (solver.nx(), solver.ny());
    for_each_pattern_stamp_cell(nx, ny, fit, emitter.pos, radius, |i, j| {
        solver.add_color_at_cell(i, j, color[0], color[1], color[2]);
        if with_force {
            solver.add_force_at_cell(i, j, Q32::from_f32_wrapping(vx), Q32::from_f32_wrapping(vy));
        }
    });
}

/// The grid cells (1-based `(i, j)`, row 1 at the top) a pattern-space stamp
/// at `pos` of `radius` covers on an `nx` × `ny` grid: those whose centre
/// `((i − 0.5) / nx, (j − 0.5) / ny)` lies inside the stamp's ellipse in grid
/// uv, plus the cell holding `pos` when it is on the grid. Each once.
pub fn for_each_pattern_stamp_cell(
    nx: usize,
    ny: usize,
    fit: &PatternGridFit,
    pos: [f32; 2],
    radius: f32,
    mut stamp: impl FnMut(usize, usize),
) {
    if nx == 0 || ny == 0 || !(pos[0].is_finite() && pos[1].is_finite()) || !(radius > 0.0) {
        return;
    }
    let [cx, cy] = fit.pattern_to_uv(pos);
    let [rx, ry] = fit.radius_to_uv(radius);
    let (nxf, nyf) = (nx as f32, ny as f32);
    let home = ((0.0..=1.0).contains(&cx) && (0.0..=1.0).contains(&cy)).then(|| {
        (
            ((cx * nxf) as usize + 1).min(nx),
            ((cy * nyf) as usize + 1).min(ny),
        )
    });
    // Cell index range whose centres can fall inside the ellipse's box.
    let range = |centre: f32, reach: f32, n: f32, len: usize| {
        let lo = libm::ceilf((centre - reach) * n + 0.5).max(1.0);
        let hi = libm::floorf((centre + reach) * n + 0.5).min(len as f32);
        (lo as usize, hi as usize)
    };
    let (i_lo, i_hi) = range(cx, rx, nxf, nx);
    let (j_lo, j_hi) = range(cy, ry, nyf, ny);
    for j in j_lo..=j_hi {
        let dv = ((j as f32 - 0.5) / nyf - cy) / ry;
        for i in i_lo..=i_hi {
            let du = ((i as f32 - 0.5) / nxf - cx) / rx;
            if du * du + dv * dv <= 1.0 && home != Some((i, j)) {
                stamp(i, j);
            }
        }
    }
    if let Some((i, j)) = home {
        stamp(i, j);
    }
}

fn grid_cell_for(solver: &MsaFluidSolver, x: f32, y: f32) -> Option<(usize, usize)> {
    let nx = solver.nx();
    let ny = solver.ny();
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    let i = (x * nx as f32) as usize + 1;
    let j = (y * ny as f32) as usize + 1;
    Some((i.min(nx), j.min(ny)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn stamp_emitter_changes_solver_after_update() {
        let mut solver = MsaFluidSolver::new(8, 8);
        solver.set_solver_iterations(1);
        let mut emitter = FluidEmitter::new(1);
        emitter.radius = 0.2;
        emitter.intensity = 2.0;
        stamp_emitter(&mut solver, &emitter);
        solver.update();

        assert!(solver.r().iter().any(|value| *value > Q32::ZERO));
    }

    /// An emitter at pattern (0, 0) lands in the lamp box's centre cell.
    #[test]
    fn a_pattern_origin_emitter_lands_in_the_centre_cell() {
        let fit = PatternGridFit::new([1.0, 0.5]);
        assert_eq!(stamped(9, 5, &fit, [0.0, 0.0], 0.01), [(5, 3)]);
    }

    /// An emitter at `(extent.x, 0)` lands on the right edge, mid-height.
    #[test]
    fn a_pattern_emitter_at_extent_x_lands_on_the_right_edge() {
        let fit = PatternGridFit::new([1.0, 0.5]);
        assert_eq!(stamped(9, 5, &fit, [1.0, 0.0], 0.01), [(9, 3)]);
        assert_eq!(stamped(9, 5, &fit, [-1.0, 0.0], 0.01), [(1, 3)]);
    }

    /// Pattern y is up: `+extent.y` is the grid's top row (row 1).
    #[test]
    fn pattern_y_is_up() {
        let fit = PatternGridFit::new([1.0, 0.5]);
        assert_eq!(stamped(9, 5, &fit, [0.0, 0.5], 0.01), [(5, 1)]);
        assert_eq!(stamped(9, 5, &fit, [0.0, -0.5], 0.01), [(5, 5)]);
    }

    /// A round radius on a 2:1 lamp box covers equal lamp distances on both
    /// axes — here on a square grid, whose cells are twice as wide as tall in
    /// pattern units, so the stamp is two cells each way across and four up
    /// and down.
    #[test]
    fn a_round_pattern_radius_covers_equal_lamp_distances_on_both_axes() {
        let fit = PatternGridFit::new([1.0, 0.5]);
        let (nx, ny) = (10, 10);
        // Centre on cell (5, 5)'s centre, uv (0.45, 0.45), so the lattice is
        // symmetric about the emitter.
        let pos = [(0.45 - 0.5) / 0.5, 0.5 - 0.45];
        let radius = 0.41;
        let to_pattern = |i: usize, j: usize| {
            let u = (i as f32 - 0.5) / nx as f32;
            let v = (j as f32 - 0.5) / ny as f32;
            [(u - 0.5) / 0.5, 0.5 - v]
        };
        let cells = stamped(nx, ny, &fit, pos, radius);
        let (mut reach_x, mut reach_y) = (0.0f32, 0.0f32);
        for (i, j) in &cells {
            let [x, y] = to_pattern(*i, *j);
            let (dx, dy) = (x - pos[0], y - pos[1]);
            assert!(
                dx * dx + dy * dy <= radius * radius + 1e-5,
                "cell ({i}, {j}) at ({dx}, {dy}) is outside the round stamp"
            );
            reach_x = reach_x.max(dx.abs());
            reach_y = reach_y.max(dy.abs());
        }
        assert!((reach_x - 0.4).abs() < 1e-5, "x reach {reach_x}");
        assert!((reach_y - 0.4).abs() < 1e-5, "y reach {reach_y}");
        let row: Vec<usize> = cells.iter().filter(|c| c.1 == 5).map(|c| c.0).collect();
        let column: Vec<usize> = cells.iter().filter(|c| c.0 == 5).map(|c| c.1).collect();
        assert_eq!((row.len(), column.len()), (5, 9), "{cells:?}");
    }

    /// The suspicion from the code read, pinned: pixels mode stamps an
    /// emitter at uv `p` into the cell whose span holds `p` (centred at
    /// `(i − 0.5) / n`), but its bilinear sampler reads that cell's value at
    /// `(i − 1) / n` — the dye comes back half a cell up and left of where it
    /// was stamped. Pixels mode keeps it (byte-identical); pattern mode's
    /// centred sampler reads the cell at its centre.
    #[test]
    fn pixels_mode_reads_its_dye_half_a_cell_up_and_left() {
        use super::super::{sample_rgb_bilinear_centred_q16, sample_rgb_bilinear_q16};

        let mut solver = MsaFluidSolver::new(8, 8);
        solver.set_solver_iterations(1);
        // An emitter at uv (0.4375, 0.4375) — cell 4's centre — stamps cell 4.
        let centre = 0.4375f32;
        assert_eq!(grid_cell_for(&solver, centre, centre), Some((4, 4)));
        solver.add_color_at_cell(4, 4, Q32::ONE, Q32::ZERO, Q32::ZERO);
        solver.update();
        let peak = solver.r()[4 + 4 * solver.stride()];
        assert!(peak > Q32::ZERO);

        let q = |uv: f32| (uv * 65536.0) as i32;
        let corner = 0.375f32;
        let (at_corner, _, _) = sample_rgb_bilinear_q16(&solver, q(corner), q(corner));
        let (at_centre, _, _) = sample_rgb_bilinear_q16(&solver, q(centre), q(centre));
        assert_eq!(at_corner, peak, "pixels mode reads cell 4 at its corner");
        assert!(at_centre < peak, "…and not at its centre");

        let (centred, _, _) = sample_rgb_bilinear_centred_q16(&solver, q(centre), q(centre));
        assert_eq!(centred, peak, "pattern mode reads cell 4 at its centre");
    }

    fn stamped(
        nx: usize,
        ny: usize,
        fit: &PatternGridFit,
        pos: [f32; 2],
        radius: f32,
    ) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();
        for_each_pattern_stamp_cell(nx, ny, fit, pos, radius, |i, j| cells.push((i, j)));
        cells.sort_unstable();
        cells
    }
}
