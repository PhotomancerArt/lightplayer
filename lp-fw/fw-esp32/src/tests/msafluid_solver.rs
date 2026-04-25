//! MSAFluid (Stam) solver, RGB dye channels, ported to no_std Rust + Q32.
//!
//! This is a throwaway perf experiment — *not* product code. It exists
//! to establish the theoretical upper bound for "fluid sim on esp32c6"
//! when implemented as native Rust + Q32 (no JIT, no engine pipeline).
//!
//! Algorithm reference:
//!   Memo Akten's MSAFluidSolver2D (lp2014, mono path), itself a port
//!   of Jos Stam, "Real-Time Fluid Dynamics for Games", GDC 2003.
//!
//! See docs/plans/2026-04-20-fluid-perf-experiment/00-notes.md for
//! the full context and the motivating engine-pipeline architecture
//! discussion.
//!
//! ## Scratch layout
//!
//! `project()` mirrors `MSAFluidSolver2D.project(u, v, uOld, vOld)` in
//! the Java reference: `u_old` holds pressure `p`, `v_old` holds
//! divergence `div` during the projection step. `p` and `div` are
//! cleared/overwritten inside `project`; the same buffers are reused
//! later as velocity sources (`add_source_*` / `fade_r`).

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use lps_q32::Q32;

/// lp2014 / Stam default Jacobi iteration count.
pub const DEFAULT_SOLVER_ITERATIONS: usize = 10;

/// Stam-style fluid solver (three dye channels).
pub struct MsaFluidSolver {
    nx: usize,
    ny: usize,
    stride: usize, // (nx + 2)
    num_cells: usize,
    dt: Q32,
    visc: Q32,
    fade_speed: Q32,
    solver_iterations: usize,

    r: Vec<Q32>,
    r_old: Vec<Q32>,
    g: Vec<Q32>,
    g_old: Vec<Q32>,
    b: Vec<Q32>,
    b_old: Vec<Q32>,
    u: Vec<Q32>,
    u_old: Vec<Q32>,
    v: Vec<Q32>,
    v_old: Vec<Q32>,
}

impl MsaFluidSolver {
    /// Create a solver for an `nx`×`ny` interior grid (plus ghost cells).
    pub fn new(nx: usize, ny: usize) -> Self {
        let stride = nx + 2;
        let num_cells = stride * (ny + 2);
        let zeros = vec![Q32::ZERO; num_cells];

        let s = Self {
            nx,
            ny,
            stride,
            num_cells,
            dt: Q32::ONE,
            visc: Q32::from_f32_wrapping(0.0001),
            fade_speed: Q32::ZERO,
            solver_iterations: DEFAULT_SOLVER_ITERATIONS,
            r: zeros.clone(),
            r_old: zeros.clone(),
            g: zeros.clone(),
            g_old: zeros.clone(),
            b: zeros.clone(),
            b_old: zeros.clone(),
            u: zeros.clone(),
            u_old: zeros.clone(),
            v: zeros.clone(),
            v_old: zeros,
        };
        s
    }

    /// Inject velocity impulse into the force buffer (before `update()`).
    pub fn add_force_at_cell(&mut self, i: usize, j: usize, vx: Q32, vy: Q32) {
        if i < 1 || i > self.nx || j < 1 || j > self.ny {
            return;
        }
        let k = idx(i, j, self.stride);
        let nx_q = Q32::from_i32(self.nx as i32);
        let ny_q = Q32::from_i32(self.ny as i32);
        self.u_old[k] = self.u_old[k] + vx * nx_q;
        self.v_old[k] = self.v_old[k] + vy * ny_q;
    }

    /// Override the number of Jacobi iterations used by the linear solvers.
    /// lp2014 default is `DEFAULT_SOLVER_ITERATIONS` (10). Lower values are
    /// cheaper but produce a less divergence-free pressure field and a less
    /// converged diffusion step.
    pub fn set_solver_iterations(&mut self, iters: usize) {
        self.solver_iterations = iters;
    }

    /// Per-step dye decay factor in `[0, 1]`. Each step, every dye cell is
    /// multiplied by `1 - fade_speed`. lp2014 default is `0.01` (1% per
    /// step); range `0.001..=0.3`. The internal default is `0` (no decay)
    /// so the perf test characterises the solver in isolation; the live
    /// demo must set this explicitly or dye saturates the field to white.
    pub fn set_fade_speed(&mut self, fade_speed: Q32) {
        self.fade_speed = fade_speed;
    }

    /// Kinematic viscosity of the velocity field. lp2014 default is
    /// `0.0001`; higher values damp velocity faster (smoother, less
    /// chaotic flow), lower values let small eddies persist. Affects the
    /// `diffuse_uv` Jacobi solve, so the cycle cost is independent of the
    /// value.
    pub fn set_viscosity(&mut self, viscosity: Q32) {
        self.visc = viscosity;
    }

    /// Add dye into the source buffer (before `update()`).
    pub fn add_color_at_cell(&mut self, i: usize, j: usize, r: Q32, g: Q32, b: Q32) {
        if i < 1 || i > self.nx || j < 1 || j > self.ny {
            return;
        }
        let k = idx(i, j, self.stride);
        self.r_old[k] = self.r_old[k] + r;
        self.g_old[k] = self.g_old[k] + g;
        self.b_old[k] = self.b_old[k] + b;
    }

    /// Advance one simulation step (RGB dye).
    pub fn update(&mut self) {
        self.add_source_uv();
        self.swap_u();
        self.swap_v();

        self.diffuse_uv();
        self.project();

        self.swap_u();
        self.swap_v();

        self.advect_u();
        self.advect_v();
        self.project();

        self.add_source_r();
        self.add_source_g();
        self.add_source_b();
        self.swap_r();
        self.swap_g();
        self.swap_b();

        self.diffuse_r();
        self.swap_r();
        self.diffuse_g();
        self.swap_g();
        self.diffuse_b();
        self.swap_b();

        self.advect_r();
        self.advect_g();
        self.advect_b();

        self.fade_r();
        self.fade_g();
        self.fade_b();
    }

    /// Red dye field (read-only).
    pub fn r(&self) -> &[Q32] {
        &self.r
    }

    pub fn g(&self) -> &[Q32] {
        &self.g
    }

    pub fn b(&self) -> &[Q32] {
        &self.b
    }

    pub fn nx(&self) -> usize {
        self.nx
    }

    pub fn ny(&self) -> usize {
        self.ny
    }

    pub fn stride(&self) -> usize {
        self.stride
    }
}

impl MsaFluidSolver {
    fn add_source_uv(&mut self) {
        for i in 0..self.num_cells {
            self.u[i] = self.u[i] + self.dt * self.u_old[i];
            self.v[i] = self.v[i] + self.dt * self.v_old[i];
        }
    }

    fn add_source_r(&mut self) {
        for i in 0..self.num_cells {
            self.r[i] = self.r[i] + self.dt * self.r_old[i];
        }
    }

    fn add_source_g(&mut self) {
        for i in 0..self.num_cells {
            self.g[i] = self.g[i] + self.dt * self.g_old[i];
        }
    }

    fn add_source_b(&mut self) {
        for i in 0..self.num_cells {
            self.b[i] = self.b[i] + self.dt * self.b_old[i];
        }
    }

    fn swap_u(&mut self) {
        core::mem::swap(&mut self.u, &mut self.u_old);
    }

    fn swap_v(&mut self) {
        core::mem::swap(&mut self.v, &mut self.v_old);
    }

    fn swap_r(&mut self) {
        core::mem::swap(&mut self.r, &mut self.r_old);
    }

    fn swap_g(&mut self) {
        core::mem::swap(&mut self.g, &mut self.g_old);
    }

    fn swap_b(&mut self) {
        core::mem::swap(&mut self.b, &mut self.b_old);
    }

    fn diffuse_uv(&mut self) {
        let nx_q = Q32::from_i32(self.nx as i32);
        let ny_q = Q32::from_i32(self.ny as i32);
        let a = self.dt * self.visc * nx_q * ny_q;
        let c = Q32::ONE + Q32::from_i32(4) * a;
        let inv_c = Q32::ONE / c;
        linear_solver_uv(
            self.nx,
            self.ny,
            self.stride,
            &mut self.u,
            &mut self.v,
            &self.u_old,
            &self.v_old,
            a,
            inv_c,
            self.solver_iterations,
        );
    }

    fn diffuse_r(&mut self) {
        let nx_q = Q32::from_i32(self.nx as i32);
        let ny_q = Q32::from_i32(self.ny as i32);
        let a = self.dt * Q32::ZERO * nx_q * ny_q;
        let c = Q32::ONE + Q32::from_i32(4) * a;
        let inv_c = Q32::ONE / c;
        linear_solver(
            BoundaryKind::None,
            &mut self.r,
            &self.r_old,
            a,
            inv_c,
            self.nx,
            self.ny,
            self.stride,
            self.solver_iterations,
        );
    }

    fn diffuse_g(&mut self) {
        let nx_q = Q32::from_i32(self.nx as i32);
        let ny_q = Q32::from_i32(self.ny as i32);
        let a = self.dt * Q32::ZERO * nx_q * ny_q;
        let c = Q32::ONE + Q32::from_i32(4) * a;
        let inv_c = Q32::ONE / c;
        linear_solver(
            BoundaryKind::None,
            &mut self.g,
            &self.g_old,
            a,
            inv_c,
            self.nx,
            self.ny,
            self.stride,
            self.solver_iterations,
        );
    }

    fn diffuse_b(&mut self) {
        let nx_q = Q32::from_i32(self.nx as i32);
        let ny_q = Q32::from_i32(self.ny as i32);
        let a = self.dt * Q32::ZERO * nx_q * ny_q;
        let c = Q32::ONE + Q32::from_i32(4) * a;
        let inv_c = Q32::ONE / c;
        linear_solver(
            BoundaryKind::None,
            &mut self.b,
            &self.b_old,
            a,
            inv_c,
            self.nx,
            self.ny,
            self.stride,
            self.solver_iterations,
        );
    }

    fn project(&mut self) {
        let nx_q = Q32::from_i32(self.nx as i32);
        let neg_inv_2nx = Q32::from_f32_wrapping(-0.5) / nx_q;
        let half_nx = Q32::from_f32_wrapping(0.5) * nx_q;
        let stride = self.stride;

        {
            let div = &mut self.v_old;
            let p = &mut self.u_old;
            for j in 1..=self.ny {
                for i in 1..=self.nx {
                    let c = idx(i, j, stride);
                    let div_val = (self.u[idx(i + 1, j, stride)] - self.u[idx(i - 1, j, stride)]
                        + self.v[idx(i, j + 1, stride)]
                        - self.v[idx(i, j - 1, stride)])
                        * neg_inv_2nx;
                    div[c] = div_val;
                    p[c] = Q32::ZERO;
                }
            }
        }

        set_boundary(
            BoundaryKind::None,
            &mut self.v_old,
            self.nx,
            self.ny,
            stride,
        );
        set_boundary(
            BoundaryKind::None,
            &mut self.u_old,
            self.nx,
            self.ny,
            stride,
        );

        let inv_c_p = Q32::ONE / Q32::from_i32(4);
        linear_solver(
            BoundaryKind::None,
            &mut self.u_old,
            &self.v_old,
            Q32::ONE,
            inv_c_p,
            self.nx,
            self.ny,
            self.stride,
            self.solver_iterations,
        );

        let p = &self.u_old;
        for j in 1..=self.ny {
            for i in 1..=self.nx {
                let c = idx(i, j, stride);
                self.u[c] =
                    self.u[c] - half_nx * (p[idx(i + 1, j, stride)] - p[idx(i - 1, j, stride)]);
                self.v[c] =
                    self.v[c] - half_nx * (p[idx(i, j + 1, stride)] - p[idx(i, j - 1, stride)]);
            }
        }

        set_boundary(BoundaryKind::MirrorX, &mut self.u, self.nx, self.ny, stride);
        set_boundary(BoundaryKind::MirrorY, &mut self.v, self.nx, self.ny, stride);
    }

    fn advect_u(&mut self) {
        let nx = self.nx;
        let ny = self.ny;
        let stride = self.stride;
        let dt = self.dt;
        advect_field(
            BoundaryKind::MirrorX,
            &mut self.u,
            &self.u_old,
            &self.u_old,
            &self.v_old,
            nx,
            ny,
            stride,
            dt,
        );
    }

    fn advect_v(&mut self) {
        let nx = self.nx;
        let ny = self.ny;
        let stride = self.stride;
        let dt = self.dt;
        advect_field(
            BoundaryKind::MirrorY,
            &mut self.v,
            &self.v_old,
            &self.u_old,
            &self.v_old,
            nx,
            ny,
            stride,
            dt,
        );
    }

    fn advect_r(&mut self) {
        let nx = self.nx;
        let ny = self.ny;
        let stride = self.stride;
        let dt = self.dt;
        advect_field(
            BoundaryKind::None,
            &mut self.r,
            &self.r_old,
            &self.u,
            &self.v,
            nx,
            ny,
            stride,
            dt,
        );
    }

    fn advect_g(&mut self) {
        let nx = self.nx;
        let ny = self.ny;
        let stride = self.stride;
        let dt = self.dt;
        advect_field(
            BoundaryKind::None,
            &mut self.g,
            &self.g_old,
            &self.u,
            &self.v,
            nx,
            ny,
            stride,
            dt,
        );
    }

    fn advect_b(&mut self) {
        let nx = self.nx;
        let ny = self.ny;
        let stride = self.stride;
        let dt = self.dt;
        advect_field(
            BoundaryKind::None,
            &mut self.b,
            &self.b_old,
            &self.u,
            &self.v,
            nx,
            ny,
            stride,
            dt,
        );
    }

    fn fade_r(&mut self) {
        let hold_amount = Q32::ONE - self.fade_speed;
        for i in 0..self.num_cells {
            self.u_old[i] = Q32::ZERO;
            self.v_old[i] = Q32::ZERO;
            self.r_old[i] = Q32::ZERO;
            if self.r[i] > Q32::ONE {
                self.r[i] = Q32::ONE;
            }
            self.r[i] = self.r[i] * hold_amount;
        }
    }

    fn fade_g(&mut self) {
        let hold_amount = Q32::ONE - self.fade_speed;
        for i in 0..self.num_cells {
            self.u_old[i] = Q32::ZERO;
            self.v_old[i] = Q32::ZERO;
            self.g_old[i] = Q32::ZERO;
            if self.g[i] > Q32::ONE {
                self.g[i] = Q32::ONE;
            }
            self.g[i] = self.g[i] * hold_amount;
        }
    }

    fn fade_b(&mut self) {
        let hold_amount = Q32::ONE - self.fade_speed;
        for i in 0..self.num_cells {
            self.u_old[i] = Q32::ZERO;
            self.v_old[i] = Q32::ZERO;
            self.b_old[i] = Q32::ZERO;
            if self.b[i] > Q32::ONE {
                self.b[i] = Q32::ONE;
            }
            self.b[i] = self.b[i] * hold_amount;
        }
    }
}

#[derive(Copy, Clone)]
enum BoundaryKind {
    None,
    MirrorX,
    MirrorY,
}

fn advect_field(
    boundary: BoundaryKind,
    d: &mut [Q32],
    d0: &[Q32],
    du: &[Q32],
    dv: &[Q32],
    nx: usize,
    ny: usize,
    stride: usize,
    dt: Q32,
) {
    let dt0 = dt * Q32::from_i32(nx as i32);
    let nx_q = Q32::from_i32(nx as i32);
    let ny_q = Q32::from_i32(ny as i32);
    let x_lo = Q32::HALF;
    let x_hi = nx_q + Q32::HALF;
    let y_lo = Q32::HALF;
    let y_hi = ny_q + Q32::HALF;

    for j in 1..=ny {
        for i in 1..=nx {
            let c = idx(i, j, stride);
            let mut x = Q32::from_i32(i as i32) - dt0 * du[c];
            let mut y = Q32::from_i32(j as i32) - dt0 * dv[c];

            if x > x_hi {
                x = x_hi;
            }
            if x < x_lo {
                x = x_lo;
            }
            debug_assert!(x.to_fixed() >= 0);

            let i0 = x.to_i32();
            let i1 = i0 + 1;

            if y > y_hi {
                y = y_hi;
            }
            if y < y_lo {
                y = y_lo;
            }
            debug_assert!(y.to_fixed() >= 0);

            let j0 = y.to_i32();
            let j1 = j0 + 1;

            let s1 = x - Q32::from_i32(i0);
            let s0 = Q32::ONE - s1;
            let t1 = y - Q32::from_i32(j0);
            let t0 = Q32::ONE - t1;

            let i0 = i0 as usize;
            let i1 = i1 as usize;
            let j0 = j0 as usize;
            let j1 = j1 as usize;

            d[c] = s0 * (t0 * d0[idx(i0, j0, stride)] + t1 * d0[idx(i0, j1, stride)])
                + s1 * (t0 * d0[idx(i1, j0, stride)] + t1 * d0[idx(i1, j1, stride)]);
        }
    }

    set_boundary(boundary, d, nx, ny, stride);
}

#[inline(always)]
fn idx(i: usize, j: usize, stride: usize) -> usize {
    i + stride * j
}

#[allow(
    clippy::too_many_arguments,
    reason = "tight inner loop; bundling into a struct adds register pressure on rv32"
)]
fn linear_solver(
    boundary: BoundaryKind,
    x: &mut [Q32],
    x0: &[Q32],
    a: Q32,
    inv_c: Q32,
    nx: usize,
    ny: usize,
    stride: usize,
    iters: usize,
) {
    for _k in 0..iters {
        for j in 1..=ny {
            for i in 1..=nx {
                let center = idx(i, j, stride);
                let neighbors =
                    x[center - 1] + x[center + 1] + x[center - stride] + x[center + stride];
                x[center] = (a * neighbors + x0[center]) * inv_c;
            }
        }
        set_boundary(boundary, x, nx, ny, stride);
    }
}

/// Linear solve for `u` and `v` simultaneously (Jacobi).
///
/// The Java reference calls `setBoundaryRGB` after each iteration; this
/// port applies [`set_boundary`] to `u` and `v` with [`BoundaryKind::None`]
/// so edge velocity samples match the scalar `setBoundary(0, …)` behavior.
#[allow(
    clippy::too_many_arguments,
    reason = "tight inner loop; bundling into a struct adds register pressure on rv32"
)]
fn linear_solver_uv(
    nx: usize,
    ny: usize,
    stride: usize,
    u: &mut [Q32],
    v: &mut [Q32],
    u0: &[Q32],
    v0: &[Q32],
    a: Q32,
    inv_c: Q32,
    iters: usize,
) {
    for _k in 0..iters {
        for j in 1..=ny {
            for i in 1..=nx {
                let center = idx(i, j, stride);
                let nu = u[center - 1] + u[center + 1] + u[center - stride] + u[center + stride];
                let nv = v[center - 1] + v[center + 1] + v[center - stride] + v[center + stride];
                u[center] = (a * nu + u0[center]) * inv_c;
                v[center] = (a * nv + v0[center]) * inv_c;
            }
        }
        set_boundary(BoundaryKind::None, u, nx, ny, stride);
        set_boundary(BoundaryKind::None, v, nx, ny, stride);
    }
}

fn set_boundary(kind: BoundaryKind, x: &mut [Q32], nx: usize, ny: usize, stride: usize) {
    let b = match kind {
        BoundaryKind::None => 0,
        BoundaryKind::MirrorX => 1,
        BoundaryKind::MirrorY => 2,
    };

    for i in 1..=nx {
        if i <= ny {
            let ix0 = idx(0, i, stride);
            let ix1 = idx(1, i, stride);
            x[ix0] = if b == 1 { -x[ix1] } else { x[ix1] };

            let ixn = idx(nx + 1, i, stride);
            let ixnm = idx(nx, i, stride);
            x[ixn] = if b == 1 { -x[ixnm] } else { x[ixnm] };
        }

        let iy0 = idx(i, 0, stride);
        let iy1 = idx(i, 1, stride);
        x[iy0] = if b == 2 { -x[iy1] } else { x[iy1] };

        let iyn = idx(i, ny + 1, stride);
        let iynm = idx(i, ny, stride);
        x[iyn] = if b == 2 { -x[iynm] } else { x[iynm] };
    }

    x[idx(0, 0, stride)] = Q32::HALF * (x[idx(1, 0, stride)] + x[idx(0, 1, stride)]);
    x[idx(0, ny + 1, stride)] = Q32::HALF * (x[idx(1, ny + 1, stride)] + x[idx(0, ny, stride)]);
    x[idx(nx + 1, 0, stride)] = Q32::HALF * (x[idx(nx, 0, stride)] + x[idx(nx + 1, 1, stride)]);
    x[idx(nx + 1, ny + 1, stride)] =
        Q32::HALF * (x[idx(nx, ny + 1, stride)] + x[idx(nx + 1, ny, stride)]);
}
