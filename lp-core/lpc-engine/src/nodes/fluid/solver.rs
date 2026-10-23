//! MSAFluid/Stam RGB dye solver used by [`super::FluidNode`].
//!
//! Algorithm reference:
//!   Memo Akten's MSAFluidSolver2D (lp2014, mono path), itself a port
//!   of Jos Stam, "Real-Time Fluid Dynamics for Games", GDC 2003.
//!
//! Provenance: derived from MSAFluidSolver2D by Memo Akten (MSAFluid).
//!   Upstream:  https://github.com/memoakten/p5-MSAFluid @
//!              6873c94c277cdd9fd45827fbe8027533cd331baa,
//!              src/msafluid/MSAFluidSolver2D.java (identical header to the
//!              lp2014 copy this was ported from:
//!              LightPlayer/PlayerCore/.../rendering/msafluid/MSAFluidSolver2D.java).
//!              The C++ port, https://github.com/memo/ofxMSAFluid @
//!              b424123652793fec721c0cb6e533936a7bd4970d, is MIT.
//!   License:   BSD-3-Clause, Copyright (c) 2008, 2009, Memo Akten,
//!              www.memo.tv. Text at licenses/MSAFluid-BSD-3-Clause.txt
//!              (and, for the C++ port, licenses/MSAFluid-MIT.txt).
//!   Here:      this file is a Rust/Q32 port and is distributed under
//!              AGPL-3.0-or-later; the upstream copyright notice, conditions
//!              and disclaimer are retained in licenses/ and must travel with
//!              any source or binary redistribution. MSA Visuals' name may
//!              not be used to endorse products derived from it.
//!              See docs/adr/2026-07-29-license-provenance-discipline.md.
//!
//! ## Scratch layout
//!
//! `project()` mirrors `MSAFluidSolver2D.project(u, v, uOld, vOld)` in
//! the Java reference: `u_old` holds pressure `p`, `v_old` holds
//! divergence `div` during the projection step. `p` and `div` are
//! cleared/overwritten inside `project`; the same buffers are reused
//! later as velocity sources (`add_source_*` / `fade`).
//!
//! ## Arithmetic
//!
//! The loops use [`add`], [`sub`] and [`mul`]: Q16.16 without saturation.
//! A fluid's values stay far inside the Q16.16 range, where these give the
//! bits `Q32`'s saturating operators give (checked against the saturating
//! solver's output when the change was made, #1282), and the saturation (an
//! i64 compare and clamp on every add) was most of a step: 53 → 20 ms at
//! 32×32×10 on the ESP32-S3, 71 → 47 ms on the C6 (S3 fluid spike,
//! 2026-10-23). A project that drove a field past ±32768 got a broken
//! picture from the saturating solver too; this one wraps instead.
//!
//! ## Passes
//!
//! The velocity's viscous solve takes one red-black pass from its source; the
//! two pressure solves take `solver_iterations` red-black passes each,
//! over-relaxed ([`over_relaxation`]). MSAFluid ran ten plain Gauss-Seidel
//! passes on every solve; at 3–4 pressure passes this looks the same (judged
//! side by side, 2026-10-23) for about half the step: S3 21 → 11 ms, C6
//! 47 → 21 ms at 32×32.

use alloc::vec;
use alloc::vec::Vec;

use lps_q32::Q32;

#[inline(always)]
fn add(a: Q32, b: Q32) -> Q32 {
    Q32(a.0.wrapping_add(b.0))
}

#[inline(always)]
fn sub(a: Q32, b: Q32) -> Q32 {
    Q32(a.0.wrapping_sub(b.0))
}

#[inline(always)]
fn mul(a: Q32, b: Q32) -> Q32 {
    Q32(((a.0 as i64 * b.0 as i64) >> Q32::SHIFT) as i32)
}

#[inline(always)]
fn neg(a: Q32) -> Q32 {
    Q32(a.0.wrapping_neg())
}

/// Pressure passes when the node sets none (lp2014 ran ten plain passes).
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

    /// Passes of each pressure solve (two a step), red-black and
    /// over-relaxed ([`over_relaxation`]). 3–4 look the same as the ten plain
    /// passes lp2014 ran; 2 and fewer show striations and harder edges. The
    /// velocity solve always takes one pass.
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
    /// `diffuse_uv` solve, so the cycle cost is independent of the
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

        // The dye has no viscosity (the reference diffuses it with 0), so
        // its diffusion solve wrote the source back on every sweep: the
        // answer is the source, already in `*_old`, with its boundary
        // rebuilt. Skipping the solve is bit-exact and was 30 of a
        // 10-iteration step's ~95 sweeps.
        let (nx, ny, stride) = (self.nx, self.ny, self.stride);
        set_boundary(BoundaryKind::None, &mut self.r_old, nx, ny, stride);
        set_boundary(BoundaryKind::None, &mut self.g_old, nx, ny, stride);
        set_boundary(BoundaryKind::None, &mut self.b_old, nx, ny, stride);

        self.advect_r();
        self.advect_g();
        self.advect_b();

        self.fade();
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
            self.u[i] = add(self.u[i], mul(self.dt, self.u_old[i]));
            self.v[i] = add(self.v[i], mul(self.dt, self.v_old[i]));
        }
    }

    fn add_source_r(&mut self) {
        for i in 0..self.num_cells {
            self.r[i] = add(self.r[i], mul(self.dt, self.r_old[i]));
        }
    }

    fn add_source_g(&mut self) {
        for i in 0..self.num_cells {
            self.g[i] = add(self.g[i], mul(self.dt, self.g_old[i]));
        }
    }

    fn add_source_b(&mut self) {
        for i in 0..self.num_cells {
            self.b[i] = add(self.b[i], mul(self.dt, self.b_old[i]));
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

    /// The viscous velocity solve: one red-black pass, started from its
    /// source. The system is nearly diagonal (a ≈ 0.03 at 32×32), so the
    /// source is almost the answer and one pass finishes it; today's ten
    /// passes from zero looked the same (S3 fluid spike, 2026-10-23).
    /// Never over-relaxed: on this system that overshoots and damps the
    /// motion.
    fn diffuse_uv(&mut self) {
        let nx_q = Q32::from_i32(self.nx as i32);
        let ny_q = Q32::from_i32(self.ny as i32);
        let a = self.dt * self.visc * nx_q * ny_q;
        let c = Q32::ONE + Q32::from_i32(4) * a;
        let inv_c = Q32::ONE / c;
        let (nx, ny, stride) = (self.nx, self.ny, self.stride);
        self.u.copy_from_slice(&self.u_old);
        self.v.copy_from_slice(&self.v_old);
        let plain = Q32::ONE;
        red_black_solver(
            BoundaryKind::None,
            &mut self.u,
            &self.u_old,
            a,
            inv_c,
            plain,
            nx,
            ny,
            stride,
            1,
        );
        red_black_solver(
            BoundaryKind::None,
            &mut self.v,
            &self.v_old,
            a,
            inv_c,
            plain,
            nx,
            ny,
            stride,
            1,
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
                    let div_val = mul(
                        sub(
                            add(
                                sub(self.u[idx(i + 1, j, stride)], self.u[idx(i - 1, j, stride)]),
                                self.v[idx(i, j + 1, stride)],
                            ),
                            self.v[idx(i, j - 1, stride)],
                        ),
                        neg_inv_2nx,
                    );
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
        red_black_solver(
            BoundaryKind::None,
            &mut self.u_old,
            &self.v_old,
            Q32::ONE,
            inv_c_p,
            over_relaxation(self.solver_iterations),
            self.nx,
            self.ny,
            self.stride,
            self.solver_iterations,
        );

        let p = &self.u_old;
        for j in 1..=self.ny {
            for i in 1..=self.nx {
                let c = idx(i, j, stride);
                self.u[c] = sub(
                    self.u[c],
                    mul(
                        half_nx,
                        sub(p[idx(i + 1, j, stride)], p[idx(i - 1, j, stride)]),
                    ),
                );
                self.v[c] = sub(
                    self.v[c],
                    mul(
                        half_nx,
                        sub(p[idx(i, j + 1, stride)], p[idx(i, j - 1, stride)]),
                    ),
                );
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

    /// Clear the sources and fade the dye: every dye cell is clamped to 1
    /// and multiplied by `1 - fade_speed`.
    fn fade(&mut self) {
        let hold_amount = Q32::ONE - self.fade_speed;
        self.u_old.fill(Q32::ZERO);
        self.v_old.fill(Q32::ZERO);
        for (d, d_old) in [
            (&mut self.r, &mut self.r_old),
            (&mut self.g, &mut self.g_old),
            (&mut self.b, &mut self.b_old),
        ] {
            d_old.fill(Q32::ZERO);
            for x in d.iter_mut() {
                if *x > Q32::ONE {
                    *x = Q32::ONE;
                }
                *x = mul(*x, hold_amount);
            }
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
            let mut x = sub(Q32::from_i32(i as i32), mul(dt0, du[c]));
            let mut y = sub(Q32::from_i32(j as i32), mul(dt0, dv[c]));

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

            let s1 = sub(x, Q32::from_i32(i0));
            let s0 = sub(Q32::ONE, s1);
            let t1 = sub(y, Q32::from_i32(j0));
            let t0 = sub(Q32::ONE, t1);

            let i0 = i0 as usize;
            let i1 = i1 as usize;
            let j0 = j0 as usize;
            let j1 = j1 as usize;

            d[c] = add(
                mul(
                    s0,
                    add(
                        mul(t0, d0[idx(i0, j0, stride)]),
                        mul(t1, d0[idx(i0, j1, stride)]),
                    ),
                ),
                mul(
                    s1,
                    add(
                        mul(t0, d0[idx(i1, j0, stride)]),
                        mul(t1, d0[idx(i1, j1, stride)]),
                    ),
                ),
            );
        }
    }

    set_boundary(boundary, d, nx, ny, stride);
}

#[inline(always)]
fn idx(i: usize, j: usize, stride: usize) -> usize {
    i + stride * j
}

/// How far each pressure pass over-relaxes, by the number of passes: fewer
/// passes lean harder. Picked by eye against the ten plain passes the solver
/// used to run (S3 fluid spike, 2026-10-23): 3 passes at 1.5 and 4 at 1.3
/// look the same as ten; 2 and fewer change the look, so they are the
/// project's call.
fn over_relaxation(passes: usize) -> Q32 {
    Q32::from_f32_wrapping(match passes {
        0..=3 => 1.5,
        4 => 1.3,
        5 | 6 => 1.2,
        7..=9 => 1.1,
        _ => 1.0,
    })
}

/// Red-black Gauss-Seidel: the red cells (i + j even), then the black ones,
/// each moved `omega` of the way to its Gauss-Seidel value (`Q32::ONE` is
/// plain red-black). Red cells read only black neighbours and the other way
/// round, so the order within a colour doesn't matter, and over-relaxing
/// converges in far fewer passes than the row-by-row sweep.
#[allow(
    clippy::too_many_arguments,
    reason = "tight inner loop; bundling into a struct adds register pressure on rv32"
)]
fn red_black_solver(
    boundary: BoundaryKind,
    x: &mut [Q32],
    x0: &[Q32],
    a: Q32,
    inv_c: Q32,
    omega: Q32,
    nx: usize,
    ny: usize,
    stride: usize,
    iters: usize,
) {
    let plain = omega == Q32::ONE;
    for _k in 0..iters {
        for parity in 0..2 {
            for j in 1..=ny {
                let mut i = 1 + ((j + parity) & 1);
                while i <= nx {
                    let center = idx(i, j, stride);
                    let neighbors = add(
                        add(add(x[center - 1], x[center + 1]), x[center - stride]),
                        x[center + stride],
                    );
                    let gs = mul(add(mul(a, neighbors), x0[center]), inv_c);
                    x[center] = if plain {
                        gs
                    } else {
                        add(x[center], mul(omega, sub(gs, x[center])))
                    };
                    i += 2;
                }
            }
        }
        set_boundary(boundary, x, nx, ny, stride);
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
            x[ix0] = if b == 1 { neg(x[ix1]) } else { x[ix1] };

            let ixn = idx(nx + 1, i, stride);
            let ixnm = idx(nx, i, stride);
            x[ixn] = if b == 1 { neg(x[ixnm]) } else { x[ixnm] };
        }

        let iy0 = idx(i, 0, stride);
        let iy1 = idx(i, 1, stride);
        x[iy0] = if b == 2 { neg(x[iy1]) } else { x[iy1] };

        let iyn = idx(i, ny + 1, stride);
        let iynm = idx(i, ny, stride);
        x[iyn] = if b == 2 { neg(x[iynm]) } else { x[iynm] };
    }

    x[idx(0, 0, stride)] = mul(Q32::HALF, add(x[idx(1, 0, stride)], x[idx(0, 1, stride)]));
    x[idx(0, ny + 1, stride)] = mul(
        Q32::HALF,
        add(x[idx(1, ny + 1, stride)], x[idx(0, ny, stride)]),
    );
    x[idx(nx + 1, 0, stride)] = mul(
        Q32::HALF,
        add(x[idx(nx, 0, stride)], x[idx(nx + 1, 1, stride)]),
    );
    x[idx(nx + 1, ny + 1, stride)] = mul(
        Q32::HALF,
        add(x[idx(nx, ny + 1, stride)], x[idx(nx + 1, ny, stride)]),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::fluid::stamp_emitter;
    use lpc_model::FluidEmitter;

    /// The solver's exact output, pinned. These sums are the spike's
    /// "1 velocity + N pressure" solver (`claude/s3-fluid-spike`,
    /// `spike::fast(n, over_relaxation(n))`) on this script, the variant Yona
    /// judged by eye on 2026-10-23; any change to the bits fails here.
    #[test]
    fn output_is_pinned_bit_for_bit() {
        let cases: [(usize, usize, f32, u32, u32); 4] = [
            // (grid, iterations, emitter velocity, sum @ 60, sum @ 300)
            (16, 3, 0.01, 0x009fb211, 0xd0c8db1f),
            (32, 10, 0.01, 0x829f03ac, 0xd4e2e08e),
            (24, 10, 0.05, 0x902ad363, 0x58557602),
            (32, 1, 0.2, 0xd106ef72, 0x863e3f31),
        ];
        let mut got = std::vec::Vec::new();
        for (n, iters, velocity, _, _) in cases {
            let mut s = MsaFluidSolver::new(n, n);
            s.set_solver_iterations(iters);
            s.set_fade_speed(Q32::from_f32_wrapping(0.08));
            s.set_viscosity(Q32::from_f32_wrapping(3e-5));
            let mut sums = [0u32; 2];
            for step in 0..300u32 {
                for e in scripted(step, velocity).iter() {
                    stamp_emitter(&mut s, e);
                }
                s.update();
                if step == 59 {
                    sums[0] = dye_sum(&s);
                }
            }
            sums[1] = dye_sum(&s);
            got.push((n, iters, velocity, sums[0], sums[1]));
        }
        assert_eq!(got, cases);
    }

    fn scripted(step: u32, velocity: f32) -> [FluidEmitter; 3] {
        let t = step as f32 / 25.0;
        let mk = |id: u32, phase: f32, color: [f32; 3]| {
            let mut e = FluidEmitter::new(id);
            let a = t * 0.7 + phase;
            e.pos = [0.5 + 0.35 * libm::cosf(a), 0.5 + 0.35 * libm::sinf(a * 1.3)];
            e.dir = [0.5 - e.pos[0], 0.5 - e.pos[1]];
            e.radius = 0.12;
            e.color = color;
            e.velocity = velocity;
            e.intensity = 0.4;
            e
        };
        [
            mk(1, 0.0, [1.0, 0.25, 0.05]),
            mk(2, 2.1, [0.05, 0.45, 1.0]),
            mk(3, 4.2, [0.1, 1.0, 0.25]),
        ]
    }

    fn dye_sum(s: &MsaFluidSolver) -> u32 {
        let mut h: u32 = 0x811c_9dc5;
        for f in [s.r(), s.g(), s.b()] {
            for x in f {
                for byte in x.0.to_le_bytes() {
                    h ^= byte as u32;
                    h = h.wrapping_mul(0x0100_0193);
                }
            }
        }
        h
    }
}
