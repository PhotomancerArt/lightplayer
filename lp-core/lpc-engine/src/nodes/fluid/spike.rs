//! SPIKE (branch `claude/s3-fluid-spike`, never merged): variant fluid
//! solvers for the S3/C6 fluid speed spike. The same Stam step as
//! `solver.rs`, generic over the number type, with one switch per speed
//! step so a bench can price each one alone.
//!
//! Derived from MSAFluidSolver2D:
//!
//! Copyright (c) 2008, 2009, Memo Akten, www.memo.tv
//! All rights reserved. Redistribution and use in source and binary forms,
//! with or without modification, are permitted provided that the following
//! conditions are met: redistributions of source code must retain the above
//! copyright notice, this list of conditions and the following disclaimer;
//! redistributions in binary form must reproduce the above copyright notice,
//! this list of conditions and the following disclaimer in the documentation
//! and/or other materials provided with the distribution; neither the name
//! of MSA Visuals nor the names of its contributors may be used to endorse or
//! promote products derived from this software without specific prior
//! written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND
//! CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES ARE DISCLAIMED.
//!
//! Itself after Jos Stam, "Real-Time Fluid Dynamics for Games", GDC 2003.

use alloc::vec;
use alloc::vec::Vec;

use lpc_model::FluidEmitter;
use lps_q32::Q32;

/// The number a field holds.
pub trait Num: Copy + PartialOrd {
    const ZERO: Self;
    const ONE: Self;
    const HALF: Self;
    fn add(self, o: Self) -> Self;
    fn sub(self, o: Self) -> Self;
    fn mul(self, o: Self) -> Self;
    fn div(self, o: Self) -> Self;
    fn neg(self) -> Self;
    fn from_i32(i: i32) -> Self;
    fn from_f32(f: f32) -> Self;
    /// Floor for the non-negative values advect gives it.
    fn floor_i32(self) -> i32;
    fn to_q32(self) -> Q32;
}

impl Num for Q32 {
    const ZERO: Self = Q32::ZERO;
    const ONE: Self = Q32::ONE;
    const HALF: Self = Q32::HALF;
    #[inline(always)]
    fn add(self, o: Self) -> Self {
        self + o
    }
    #[inline(always)]
    fn sub(self, o: Self) -> Self {
        self - o
    }
    #[inline(always)]
    fn mul(self, o: Self) -> Self {
        self * o
    }
    #[inline(always)]
    fn div(self, o: Self) -> Self {
        self / o
    }
    #[inline(always)]
    fn neg(self) -> Self {
        -self
    }
    #[inline(always)]
    fn from_i32(i: i32) -> Self {
        Q32::from_i32(i)
    }
    #[inline(always)]
    fn from_f32(f: f32) -> Self {
        Q32::from_f32_wrapping(f)
    }
    #[inline(always)]
    fn floor_i32(self) -> i32 {
        self.to_i32()
    }
    #[inline(always)]
    fn to_q32(self) -> Q32 {
        self
    }
}

/// Q16.16 with wrapping add/sub and an unsaturated multiply: the same bits
/// as [`Q32`] wherever nothing overflows (a fluid's values never come close).
#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub struct Qw(pub i32);

impl Num for Qw {
    const ZERO: Self = Qw(0);
    const ONE: Self = Qw(1 << 16);
    const HALF: Self = Qw(1 << 15);
    #[inline(always)]
    fn add(self, o: Self) -> Self {
        Qw(self.0.wrapping_add(o.0))
    }
    #[inline(always)]
    fn sub(self, o: Self) -> Self {
        Qw(self.0.wrapping_sub(o.0))
    }
    #[inline(always)]
    fn mul(self, o: Self) -> Self {
        Qw(((self.0 as i64 * o.0 as i64) >> 16) as i32)
    }
    #[inline(always)]
    fn div(self, o: Self) -> Self {
        Qw((Q32(self.0) / Q32(o.0)).0)
    }
    #[inline(always)]
    fn neg(self) -> Self {
        Qw(self.0.wrapping_neg())
    }
    #[inline(always)]
    fn from_i32(i: i32) -> Self {
        Qw(i << 16)
    }
    #[inline(always)]
    fn from_f32(f: f32) -> Self {
        Qw(Q32::from_f32_wrapping(f).0)
    }
    #[inline(always)]
    fn floor_i32(self) -> i32 {
        self.0 >> 16
    }
    #[inline(always)]
    fn to_q32(self) -> Q32 {
        Q32(self.0)
    }
}

impl Num for f32 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    const HALF: Self = 0.5;
    #[inline(always)]
    fn add(self, o: Self) -> Self {
        self + o
    }
    #[inline(always)]
    fn sub(self, o: Self) -> Self {
        self - o
    }
    #[inline(always)]
    fn mul(self, o: Self) -> Self {
        self * o
    }
    #[inline(always)]
    fn div(self, o: Self) -> Self {
        self / o
    }
    #[inline(always)]
    fn neg(self) -> Self {
        -self
    }
    #[inline(always)]
    fn from_i32(i: i32) -> Self {
        i as f32
    }
    #[inline(always)]
    fn from_f32(f: f32) -> Self {
        f
    }
    #[inline(always)]
    fn floor_i32(self) -> i32 {
        self as i32
    }
    #[inline(always)]
    fn to_q32(self) -> Q32 {
        Q32::from_f32_wrapping(self)
    }
}

/// How a linear solve sweeps.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Sweep {
    /// Today's: in place, row by row (Gauss-Seidel).
    Gs,
    /// Red cells then black cells, over-relaxed by `omega` (1.0 = plain
    /// red-black Gauss-Seidel).
    RedBlack { omega: f32 },
    /// Every cell from the last sweep's values (two buffers): the ordering
    /// SIMD lanes can run, slower to converge.
    Jacobi,
}

/// One speed step per switch. [`Opts::BASE`] is today's solver, bit for bit.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Opts {
    /// Skip the dye "diffusion" while the dye's viscosity is zero (it always
    /// is: `solver.rs` passes `Q32::ZERO`): ten copy sweeps per channel.
    /// Bit-exact.
    pub skip_still_dye: bool,
    /// Zero the velocity sources once per step, not three times. Bit-exact.
    pub fade_once: bool,
    pub sweep: Sweep,
    /// Sweeps of the viscous velocity diffusion (`None` = the node's count).
    pub vel_iters: Option<usize>,
    /// Sweeps of each pressure solve (`None` = the node's count).
    pub p_iters: Option<usize>,
    /// Start each pressure solve from the last step's pressure, not zero.
    pub warm_p: bool,
    /// Start the velocity diffusion from its source (the answer it is a
    /// small correction to), not from the zeroed scratch.
    pub vel_from_src: bool,
}

impl Opts {
    pub const BASE: Opts = Opts {
        skip_still_dye: false,
        fade_once: false,
        sweep: Sweep::Gs,
        vel_iters: None,
        p_iters: None,
        warm_p: false,
        vel_from_src: false,
    };
    /// The bit-exact steps.
    pub const EXACT: Opts = Opts {
        skip_still_dye: true,
        fade_once: true,
        ..Opts::BASE
    };
}

pub struct SpikeSolver<T: Num> {
    nx: usize,
    ny: usize,
    stride: usize,
    num_cells: usize,
    dt: T,
    visc: T,
    fade_speed: T,
    iters: usize,
    opts: Opts,
    pub r: Vec<T>,
    r_old: Vec<T>,
    pub g: Vec<T>,
    g_old: Vec<T>,
    pub b: Vec<T>,
    b_old: Vec<T>,
    u: Vec<T>,
    u_old: Vec<T>,
    v: Vec<T>,
    v_old: Vec<T>,
    /// The two pressure solves' last answers (`warm_p`).
    p_warm: [Vec<T>; 2],
    project_call: usize,
}

impl<T: Num> SpikeSolver<T> {
    pub fn new(nx: usize, ny: usize, iters: usize, fade: f32, visc: f32, opts: Opts) -> Self {
        let stride = nx + 2;
        let num_cells = stride * (ny + 2);
        let z = vec![T::ZERO; num_cells];
        let warm = if opts.warm_p { z.clone() } else { Vec::new() };
        Self {
            nx,
            ny,
            stride,
            num_cells,
            dt: T::ONE,
            visc: T::from_f32(visc),
            fade_speed: T::from_f32(fade),
            iters,
            opts,
            r: z.clone(),
            r_old: z.clone(),
            g: z.clone(),
            g_old: z.clone(),
            b: z.clone(),
            b_old: z.clone(),
            u: z.clone(),
            u_old: z.clone(),
            v: z.clone(),
            v_old: z,
            p_warm: [warm.clone(), warm],
            project_call: 0,
        }
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

    pub fn add_force_at_cell(&mut self, i: usize, j: usize, vx: f32, vy: f32) {
        if i < 1 || i > self.nx || j < 1 || j > self.ny {
            return;
        }
        let k = i + self.stride * j;
        let nx_q = T::from_i32(self.nx as i32);
        let ny_q = T::from_i32(self.ny as i32);
        self.u_old[k] = self.u_old[k].add(T::from_f32(vx).mul(nx_q));
        self.v_old[k] = self.v_old[k].add(T::from_f32(vy).mul(ny_q));
    }

    pub fn add_color_at_cell(&mut self, i: usize, j: usize, r: f32, g: f32, b: f32) {
        if i < 1 || i > self.nx || j < 1 || j > self.ny {
            return;
        }
        let k = i + self.stride * j;
        self.r_old[k] = self.r_old[k].add(T::from_f32(r));
        self.g_old[k] = self.g_old[k].add(T::from_f32(g));
        self.b_old[k] = self.b_old[k].add(T::from_f32(b));
    }

    pub fn update(&mut self) {
        let n = self.num_cells;
        for i in 0..n {
            self.u[i] = self.u[i].add(self.dt.mul(self.u_old[i]));
            self.v[i] = self.v[i].add(self.dt.mul(self.v_old[i]));
        }
        core::mem::swap(&mut self.u, &mut self.u_old);
        core::mem::swap(&mut self.v, &mut self.v_old);

        self.diffuse_uv();
        self.project_call = 0;
        self.project();

        core::mem::swap(&mut self.u, &mut self.u_old);
        core::mem::swap(&mut self.v, &mut self.v_old);

        let (nx, ny, s, dt) = (self.nx, self.ny, self.stride, self.dt);
        advect(Bnd::MirrorX, &mut self.u, &self.u_old, &self.u_old, &self.v_old, nx, ny, s, dt);
        advect(Bnd::MirrorY, &mut self.v, &self.v_old, &self.u_old, &self.v_old, nx, ny, s, dt);
        self.project_call = 1;
        self.project();

        for i in 0..n {
            self.r[i] = self.r[i].add(self.dt.mul(self.r_old[i]));
        }
        for i in 0..n {
            self.g[i] = self.g[i].add(self.dt.mul(self.g_old[i]));
        }
        for i in 0..n {
            self.b[i] = self.b[i].add(self.dt.mul(self.b_old[i]));
        }
        core::mem::swap(&mut self.r, &mut self.r_old);
        core::mem::swap(&mut self.g, &mut self.g_old);
        core::mem::swap(&mut self.b, &mut self.b_old);

        if self.opts.skip_still_dye {
            // a = 0, c = 1: every sweep writes x0 back. The answer is x0 with
            // its boundary rebuilt; it is read from `*_old` below.
            set_bnd(Bnd::None, &mut self.r_old, nx, ny, s);
            set_bnd(Bnd::None, &mut self.g_old, nx, ny, s);
            set_bnd(Bnd::None, &mut self.b_old, nx, ny, s);
        } else {
            let a = self.dt.mul(T::ZERO).mul(T::from_i32(nx as i32)).mul(T::from_i32(ny as i32));
            let inv_c = T::ONE.div(T::ONE.add(T::from_i32(4).mul(a)));
            let it = self.iters;
            let sw = self.opts.sweep;
            lin_solve(sw, Bnd::None, &mut self.r, &self.r_old, a, inv_c, nx, ny, s, it);
            core::mem::swap(&mut self.r, &mut self.r_old);
            lin_solve(sw, Bnd::None, &mut self.g, &self.g_old, a, inv_c, nx, ny, s, it);
            core::mem::swap(&mut self.g, &mut self.g_old);
            lin_solve(sw, Bnd::None, &mut self.b, &self.b_old, a, inv_c, nx, ny, s, it);
            core::mem::swap(&mut self.b, &mut self.b_old);
        }

        advect(Bnd::None, &mut self.r, &self.r_old, &self.u, &self.v, nx, ny, s, dt);
        advect(Bnd::None, &mut self.g, &self.g_old, &self.u, &self.v, nx, ny, s, dt);
        advect(Bnd::None, &mut self.b, &self.b_old, &self.u, &self.v, nx, ny, s, dt);

        let hold = T::ONE.sub(self.fade_speed);
        if self.opts.fade_once {
            self.u_old.fill(T::ZERO);
            self.v_old.fill(T::ZERO);
            fade(&mut self.r, &mut self.r_old, hold);
            fade(&mut self.g, &mut self.g_old, hold);
            fade(&mut self.b, &mut self.b_old, hold);
        } else {
            for (d, d_old) in [
                (&mut self.r, &mut self.r_old),
                (&mut self.g, &mut self.g_old),
                (&mut self.b, &mut self.b_old),
            ] {
                for i in 0..n {
                    self.u_old[i] = T::ZERO;
                    self.v_old[i] = T::ZERO;
                    d_old[i] = T::ZERO;
                    if d[i] > T::ONE {
                        d[i] = T::ONE;
                    }
                    d[i] = d[i].mul(hold);
                }
            }
        }
    }

    fn diffuse_uv(&mut self) {
        let nx_q = T::from_i32(self.nx as i32);
        let ny_q = T::from_i32(self.ny as i32);
        let a = self.dt.mul(self.visc).mul(nx_q).mul(ny_q);
        let inv_c = T::ONE.div(T::ONE.add(T::from_i32(4).mul(a)));
        let iters = self.opts.vel_iters.unwrap_or(self.iters);
        let (nx, ny, s) = (self.nx, self.ny, self.stride);
        if self.opts.vel_from_src {
            self.u.copy_from_slice(&self.u_old);
            self.v.copy_from_slice(&self.v_old);
        }
        match self.opts.sweep {
            Sweep::Gs => {
                for _ in 0..iters {
                    for j in 1..=ny {
                        for i in 1..=nx {
                            let c = i + s * j;
                            let nu = self.u[c - 1].add(self.u[c + 1]).add(self.u[c - s]).add(self.u[c + s]);
                            let nv = self.v[c - 1].add(self.v[c + 1]).add(self.v[c - s]).add(self.v[c + s]);
                            self.u[c] = a.mul(nu).add(self.u_old[c]).mul(inv_c);
                            self.v[c] = a.mul(nv).add(self.v_old[c]).mul(inv_c);
                        }
                    }
                    set_bnd(Bnd::None, &mut self.u, nx, ny, s);
                    set_bnd(Bnd::None, &mut self.v, nx, ny, s);
                }
            }
            Sweep::Jacobi => {
                lin_solve(Sweep::Jacobi, Bnd::None, &mut self.u, &self.u_old, a, inv_c, nx, ny, s, iters);
                lin_solve(Sweep::Jacobi, Bnd::None, &mut self.v, &self.v_old, a, inv_c, nx, ny, s, iters);
            }
            Sweep::RedBlack { .. } => {
                // Never over-relaxed: this system is nearly diagonal, and
                // over-relaxing it overshoots (it damped the motion to 30 %).
                let sw = Sweep::RedBlack { omega: 1.0 };
                lin_solve(sw, Bnd::None, &mut self.u, &self.u_old, a, inv_c, nx, ny, s, iters);
                lin_solve(sw, Bnd::None, &mut self.v, &self.v_old, a, inv_c, nx, ny, s, iters);
            }
        }
    }

    fn project(&mut self) {
        let (nx, ny, s) = (self.nx, self.ny, self.stride);
        let nx_q = T::from_i32(nx as i32);
        let neg_inv_2nx = T::from_f32(-0.5).div(nx_q);
        let half_nx = T::from_f32(0.5).mul(nx_q);
        let warm = self.opts.warm_p;
        for j in 1..=ny {
            for i in 1..=nx {
                let c = i + s * j;
                let d = self.u[c + 1]
                    .sub(self.u[c - 1])
                    .add(self.v[c + s])
                    .sub(self.v[c - s])
                    .mul(neg_inv_2nx);
                self.v_old[c] = d;
                self.u_old[c] = T::ZERO;
            }
        }
        set_bnd(Bnd::None, &mut self.v_old, nx, ny, s);
        if warm {
            core::mem::swap(&mut self.u_old, &mut self.p_warm[self.project_call]);
        } else {
            set_bnd(Bnd::None, &mut self.u_old, nx, ny, s);
        }
        let iters = self.opts.p_iters.unwrap_or(self.iters);
        let inv_c = T::ONE.div(T::from_i32(4));
        lin_solve(self.opts.sweep, Bnd::None, &mut self.u_old, &self.v_old, T::ONE, inv_c, nx, ny, s, iters);

        for j in 1..=ny {
            for i in 1..=nx {
                let c = i + s * j;
                let p = &self.u_old;
                self.u[c] = self.u[c].sub(half_nx.mul(p[c + 1].sub(p[c - 1])));
                self.v[c] = self.v[c].sub(half_nx.mul(p[c + s].sub(p[c - s])));
            }
        }
        if warm {
            // Keep the answer for the next step's same solve; the scratch
            // that comes back is zeroed by the next divergence pass.
            self.p_warm[self.project_call].copy_from_slice(&self.u_old);
        }
        set_bnd(Bnd::MirrorX, &mut self.u, nx, ny, s);
        set_bnd(Bnd::MirrorY, &mut self.v, nx, ny, s);
    }
}

#[inline(always)]
fn fade<T: Num>(d: &mut [T], d_old: &mut [T], hold: T) {
    d_old.fill(T::ZERO);
    for x in d.iter_mut() {
        if *x > T::ONE {
            *x = T::ONE;
        }
        *x = x.mul(hold);
    }
}

#[derive(Copy, Clone)]
enum Bnd {
    None,
    MirrorX,
    MirrorY,
}

#[allow(clippy::too_many_arguments)]
fn advect<T: Num>(
    bnd: Bnd,
    d: &mut [T],
    d0: &[T],
    du: &[T],
    dv: &[T],
    nx: usize,
    ny: usize,
    s: usize,
    dt: T,
) {
    let dt0 = dt.mul(T::from_i32(nx as i32));
    let x_hi = T::from_i32(nx as i32).add(T::HALF);
    let y_hi = T::from_i32(ny as i32).add(T::HALF);
    for j in 1..=ny {
        for i in 1..=nx {
            let c = i + s * j;
            let mut x = T::from_i32(i as i32).sub(dt0.mul(du[c]));
            let mut y = T::from_i32(j as i32).sub(dt0.mul(dv[c]));
            if x > x_hi {
                x = x_hi;
            }
            if x < T::HALF {
                x = T::HALF;
            }
            let i0 = x.floor_i32();
            if y > y_hi {
                y = y_hi;
            }
            if y < T::HALF {
                y = T::HALF;
            }
            let j0 = y.floor_i32();
            let s1 = x.sub(T::from_i32(i0));
            let s0 = T::ONE.sub(s1);
            let t1 = y.sub(T::from_i32(j0));
            let t0 = T::ONE.sub(t1);
            let (i0, j0) = (i0 as usize, j0 as usize);
            let k = i0 + s * j0;
            d[c] = s0
                .mul(t0.mul(d0[k]).add(t1.mul(d0[k + s])))
                .add(s1.mul(t0.mul(d0[k + 1]).add(t1.mul(d0[k + 1 + s]))));
        }
    }
    set_bnd(bnd, d, nx, ny, s);
}

#[allow(clippy::too_many_arguments)]
fn lin_solve<T: Num>(
    sweep: Sweep,
    bnd: Bnd,
    x: &mut [T],
    x0: &[T],
    a: T,
    inv_c: T,
    nx: usize,
    ny: usize,
    s: usize,
    iters: usize,
) {
    match sweep {
        Sweep::Gs => {
            for _ in 0..iters {
                for j in 1..=ny {
                    for i in 1..=nx {
                        let c = i + s * j;
                        let n = x[c - 1].add(x[c + 1]).add(x[c - s]).add(x[c + s]);
                        x[c] = a.mul(n).add(x0[c]).mul(inv_c);
                    }
                }
                set_bnd(bnd, x, nx, ny, s);
            }
        }
        Sweep::Jacobi => {
            let mut prev = x.to_vec();
            for _ in 0..iters {
                for j in 1..=ny {
                    for i in 1..=nx {
                        let c = i + s * j;
                        let n = prev[c - 1].add(prev[c + 1]).add(prev[c - s]).add(prev[c + s]);
                        x[c] = a.mul(n).add(x0[c]).mul(inv_c);
                    }
                }
                set_bnd(bnd, x, nx, ny, s);
                prev.copy_from_slice(x);
            }
        }
        Sweep::RedBlack { omega } => {
            let w = T::from_f32(omega);
            let plain = omega == 1.0;
            for _ in 0..iters {
                for parity in 0..2 {
                    for j in 1..=ny {
                        let start = 1 + ((j + parity) & 1);
                        let mut i = start;
                        while i <= nx {
                            let c = i + s * j;
                            let n = x[c - 1].add(x[c + 1]).add(x[c - s]).add(x[c + s]);
                            let gs = a.mul(n).add(x0[c]).mul(inv_c);
                            x[c] = if plain { gs } else { x[c].add(w.mul(gs.sub(x[c]))) };
                            i += 2;
                        }
                    }
                }
                set_bnd(bnd, x, nx, ny, s);
            }
        }
    }
}

fn set_bnd<T: Num>(kind: Bnd, x: &mut [T], nx: usize, ny: usize, s: usize) {
    let b = match kind {
        Bnd::None => 0,
        Bnd::MirrorX => 1,
        Bnd::MirrorY => 2,
    };
    let idx = |i: usize, j: usize| i + s * j;
    for i in 1..=nx {
        if i <= ny {
            x[idx(0, i)] = if b == 1 { x[idx(1, i)].neg() } else { x[idx(1, i)] };
            x[idx(nx + 1, i)] = if b == 1 { x[idx(nx, i)].neg() } else { x[idx(nx, i)] };
        }
        x[idx(i, 0)] = if b == 2 { x[idx(i, 1)].neg() } else { x[idx(i, 1)] };
        x[idx(i, ny + 1)] = if b == 2 { x[idx(i, ny)].neg() } else { x[idx(i, ny)] };
    }
    x[idx(0, 0)] = T::HALF.mul(x[idx(1, 0)].add(x[idx(0, 1)]));
    x[idx(0, ny + 1)] = T::HALF.mul(x[idx(1, ny + 1)].add(x[idx(0, ny)]));
    x[idx(nx + 1, 0)] = T::HALF.mul(x[idx(nx, 0)].add(x[idx(nx + 1, 1)]));
    x[idx(nx + 1, ny + 1)] = T::HALF.mul(x[idx(nx, ny + 1)].add(x[idx(nx + 1, ny)]));
}

// --- the emitter script and the stamp, shared by the bench and the look ----

/// `fluid32i10`'s compute shader in Rust: three emitters at time `t`.
pub fn script_emitters(t: f32) -> [FluidEmitter; 3] {
    const TAU: f32 = 6.283_185_3;
    let ph = |period: f32, off: f32| {
        let x = t / period + off;
        x - libm::floorf(x)
    };
    let sin = |p: f32| libm::sinf(TAU * p);
    let breathe = 0.5 + 0.5 * sin(ph(34.906_586, 0.0));
    let mk = |id: u32, pos: [f32; 2], radius: f32, color: [f32; 3], velocity: f32, intensity: f32| {
        let mut e = FluidEmitter::new(id);
        e.pos = pos;
        e.dir = [0.5 - pos[0], 0.5 - pos[1]];
        e.radius = radius;
        e.color = color;
        e.velocity = velocity;
        e.intensity = intensity;
        e
    };
    [
        mk(
            1,
            [0.12 + 0.12 * sin(ph(20.268_34, 0.0)), 0.50 + 0.34 * sin(ph(27.764_85, 0.0))],
            0.125,
            [1.0, 0.25, 0.05],
            0.01,
            0.40,
        ),
        mk(
            2,
            [
                0.88 + 0.08 * sin(ph(33.726_17, 0.270_722_6)),
                0.50 + 0.36 * sin(ph(27.318_198, 0.334_225_4)),
            ],
            0.1,
            [0.05, 0.45, 1.0],
            0.007,
            0.36,
        ),
        mk(
            3,
            [
                0.50 + 0.34 * sin(ph(33.069_397, 0.668_450_7)),
                0.12 + 0.12 * sin(ph(49.357_31, 0.447_862)),
            ],
            0.085 + breathe * 0.02,
            [0.1, 1.0, 0.25],
            0.003,
            0.32,
        ),
    ]
}

/// `emit::stamp_emitter`, for a [`SpikeSolver`].
pub fn stamp<T: Num>(solver: &mut SpikeSolver<T>, emitter: &FluidEmitter) {
    let radius = emitter.radius.max(0.0);
    if radius <= 0.0 || emitter.intensity <= 0.0 {
        return;
    }
    let dir_len2 = emitter.dir[0] * emitter.dir[0] + emitter.dir[1] * emitter.dir[1];
    let inv = if dir_len2 > 0.000001 {
        1.0 / libm::sqrtf(dir_len2)
    } else {
        0.0
    };
    let vx = emitter.dir[0] * inv * emitter.velocity * emitter.intensity;
    let vy = emitter.dir[1] * inv * emitter.velocity * emitter.intensity;
    let (nx, ny) = (solver.nx(), solver.ny());
    let (xs, ys) = (1.0 / nx as f32, 1.0 / ny as f32);
    let r2 = radius * radius;
    let mut dx = -radius;
    while dx <= radius {
        let mut dy = -radius;
        while dy <= radius {
            let (x, y) = (emitter.pos[0] + dx, emitter.pos[1] + dy);
            if dx * dx + dy * dy <= r2 && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) {
                let i = ((x * nx as f32) as usize + 1).min(nx);
                let j = ((y * ny as f32) as usize + 1).min(ny);
                let c = emitter.color;
                let k = emitter.intensity;
                solver.add_color_at_cell(i, j, c[0] * k, c[1] * k, c[2] * k);
                if inv > 0.0 && emitter.velocity != 0.0 {
                    solver.add_force_at_cell(i, j, vx, vy);
                }
            }
            dy += ys;
        }
        dx += xs;
    }
}

/// One scripted step at time `t` (25 Hz steps: `t = step / 25`).
pub fn scripted_step<T: Num>(solver: &mut SpikeSolver<T>, step: u32) {
    let t = step as f32 / 25.0;
    for e in script_emitters(t).iter() {
        stamp(solver, e);
    }
    solver.update();
}

/// FNV-1a over the dye fields as Q16.16 bits: the same on host and device.
pub fn dye_checksum<T: Num>(solver: &SpikeSolver<T>) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for f in [&solver.r, &solver.g, &solver.b] {
        for x in f.iter() {
            for byte in x.to_q32().0.to_le_bytes() {
                h ^= byte as u32;
                h = h.wrapping_mul(0x0100_0193);
            }
        }
    }
    h
}

// --- the bench --------------------------------------------------------------

/// A named bench row.
#[derive(Copy, Clone)]
pub struct Variant {
    pub name: &'static str,
    pub num: NumKind,
    pub opts: Opts,
}

#[derive(Copy, Clone, PartialEq)]
pub enum NumKind {
    /// Today's saturating Q16.16.
    Q32,
    /// Q16.16, wrapping.
    Qw,
    F32,
}

/// The fast settings, by sweeps: one velocity sweep started from its source,
/// `p` red-black pressure sweeps over-relaxed by `omega`.
pub const fn fast(p: usize, omega: f32) -> Opts {
    Opts {
        vel_iters: Some(1),
        vel_from_src: true,
        p_iters: Some(p),
        sweep: Sweep::RedBlack { omega },
        ..Opts::EXACT
    }
}

const fn row(name: &'static str, num: NumKind, opts: Opts) -> Variant {
    Variant { name, num, opts }
}

/// The rows, cheapest change first.
pub const VARIANTS: &[Variant] = &[
    row("base", NumKind::Q32, Opts::BASE),
    row("exact", NumKind::Q32, Opts::EXACT),
    row("exact-qw", NumKind::Qw, Opts::EXACT),
    row("exact-f32", NumKind::F32, Opts::EXACT),
    row("vel1-src-qw", NumKind::Qw, Opts { vel_iters: Some(1), vel_from_src: true, ..Opts::EXACT }),
    row("vel1-src-f32", NumKind::F32, Opts { vel_iters: Some(1), vel_from_src: true, ..Opts::EXACT }),
    row("p7-gs-qw", NumKind::Qw, Opts { vel_iters: Some(1), vel_from_src: true, p_iters: Some(7), ..Opts::EXACT }),
    row("p5-rb1.2-qw", NumKind::Qw, fast(5, 1.2)),
    row("p5-rb1.2-f32", NumKind::F32, fast(5, 1.2)),
    row("p4-rb1.3-qw", NumKind::Qw, fast(4, 1.3)),
    row("p4-rb1.3-f32", NumKind::F32, fast(4, 1.3)),
    row("p3-rb1.5-qw", NumKind::Qw, fast(3, 1.5)),
    row("p3-rb1.5-f32", NumKind::F32, fast(3, 1.5)),
    row("p2-rb1.5-qw", NumKind::Qw, fast(2, 1.5)),
    row("p2-rb1.5-f32", NumKind::F32, fast(2, 1.5)),
];

/// A variant behind `MsaFluidSolver` (`LP_FLUID_VARIANT`).
pub enum AnySolver {
    Q32(SpikeSolver<Q32>),
    Qw(SpikeSolver<Qw>),
    F32(SpikeSolver<f32>),
}

macro_rules! each {
    ($self:ident, $s:ident => $e:expr) => {
        match $self {
            AnySolver::Q32($s) => $e,
            AnySolver::Qw($s) => $e,
            AnySolver::F32($s) => $e,
        }
    };
}

impl AnySolver {
    pub fn new(v: &Variant, n: usize, iters: usize, fade: f32, visc: f32) -> Self {
        match v.num {
            NumKind::Q32 => AnySolver::Q32(SpikeSolver::new(n, n, iters, fade, visc, v.opts)),
            NumKind::Qw => AnySolver::Qw(SpikeSolver::new(n, n, iters, fade, visc, v.opts)),
            NumKind::F32 => AnySolver::F32(SpikeSolver::new(n, n, iters, fade, visc, v.opts)),
        }
    }
    pub fn add_force_at_cell(&mut self, i: usize, j: usize, vx: f32, vy: f32) {
        each!(self, s => s.add_force_at_cell(i, j, vx, vy))
    }
    pub fn add_color_at_cell(&mut self, i: usize, j: usize, r: f32, g: f32, b: f32) {
        each!(self, s => s.add_color_at_cell(i, j, r, g, b))
    }
    pub fn update(&mut self) {
        each!(self, s => s.update())
    }
    pub fn copy_dye(&self, r: &mut [Q32], g: &mut [Q32], b: &mut [Q32]) {
        each!(self, s => {
            for (d, x) in r.iter_mut().zip(s.r.iter()) {
                *d = x.to_q32();
            }
            for (d, x) in g.iter_mut().zip(s.g.iter()) {
                *d = x.to_q32();
            }
            for (d, x) in b.iter_mut().zip(s.b.iter()) {
                *d = x.to_q32();
            }
        })
    }
}

/// A row by name (`LP_FLUID_VARIANT`).
pub fn variant(name: &str) -> Option<&'static Variant> {
    VARIANTS.iter().find(|v| v.name == name)
}

/// One bench row: `warm` scripted steps, then `steps` timed ones.
/// `cycles` is the edge's cycle counter. Returns (cycles per step, checksum
/// after the run, address of the red field).
pub fn bench_row(
    v: &Variant,
    n: usize,
    iters: usize,
    warm: u32,
    steps: u32,
    cycles: &dyn Fn() -> u32,
) -> (u32, u32, usize) {
    fn run<T: Num>(
        o: Opts,
        n: usize,
        iters: usize,
        warm: u32,
        steps: u32,
        cycles: &dyn Fn() -> u32,
    ) -> (u32, u32, usize) {
        let mut s = SpikeSolver::<T>::new(n, n, iters, 0.08, 3e-5, o);
        for k in 0..warm {
            scripted_step(&mut s, k);
        }
        let t0 = cycles();
        for k in warm..warm + steps {
            scripted_step(&mut s, k);
        }
        let dt = cycles().wrapping_sub(t0);
        (dt / steps.max(1), dye_checksum(&s), s.r.as_ptr() as usize)
    }
    match v.num {
        NumKind::Q32 => run::<Q32>(v.opts, n, iters, warm, steps, cycles),
        NumKind::Qw => run::<Qw>(v.opts, n, iters, warm, steps, cycles),
        NumKind::F32 => run::<f32>(v.opts, n, iters, warm, steps, cycles),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::fluid::{MsaFluidSolver, stamp_emitter};

    /// `Opts::BASE` over Q32 is today's solver, bit for bit.
    #[test]
    fn base_matches_msa_solver() {
        for n in [16usize, 32] {
            let mut a = MsaFluidSolver::new(n, n);
            a.set_solver_iterations(10);
            a.set_fade_speed(Q32::from_f32_wrapping(0.08));
            a.set_viscosity(Q32::from_f32_wrapping(3e-5));
            let mut b = SpikeSolver::<Q32>::new(n, n, 10, 0.08, 3e-5, Opts::BASE);
            let mut c = SpikeSolver::<Q32>::new(n, n, 10, 0.08, 3e-5, Opts::EXACT);
            let mut d = SpikeSolver::<Qw>::new(n, n, 10, 0.08, 3e-5, Opts::EXACT);
            for step in 0..300u32 {
                for e in script_emitters(step as f32 / 25.0).iter() {
                    stamp_emitter(&mut a, e);
                }
                a.update();
                scripted_step(&mut b, step);
                scripted_step(&mut c, step);
                scripted_step(&mut d, step);
                assert!(a.r() == &b.r[..] && a.g() == &b.g[..] && a.b() == &b.b[..], "base, step {step}");
                assert!(b.r == c.r && b.g == c.g && b.b == c.b, "exact, step {step}");
                assert_eq!(dye_checksum(&c), dye_checksum(&d), "qw, step {step}");
            }
        }
    }
}
