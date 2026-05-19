//! Fluid emitter stamping.

use lpc_model::FluidEmitter;
use lps_q32::Q32;

use super::solver::MsaFluidSolver;

/// Stamp one emitter into the solver's source buffers.
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
}
