//! SPIKE (`claude/s3-fluid-spike`, never merged): the S3's PIE SIMD on the
//! fluid's pressure sweep. Run from `fluid_bench` (LP_FLUID_BENCH=1).
//!
//! 1. A probe of the two instructions whose operand order the spike could
//!    not read from the TRM: `ee.src.q` (the unaligned concat) and
//!    `ee.vsr.32` (the lane shift by SAR).
//! 2. One Jacobi pressure sweep, `p' = (pL + pR + pU + pD + div) >> 2` on
//!    Q16.16 in 32-bit lanes (four cells a vector), against the same sweep
//!    scalar, and against today's in-place Gauss-Seidel sweep. The answer is
//!    checked against the scalar Jacobi bit for bit.
//!
//! The pressure solve is the one Stam sweep with no multiply in it (a = 1,
//! 1/c = 1/4), which is what makes it a 32-bit-lane kernel: PIE has no
//! 32-bit lane multiply.

use alloc::vec;
use alloc::vec::Vec;

use crate::board::esp32s3::{constants::CPU_HZ, cycle_counter};

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct V4([i32; 4]);

fn cells(v: &mut [V4]) -> &mut [i32] {
    // SAFETY: V4 is four i32s, 16-aligned, no padding.
    unsafe { core::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut i32, v.len() * 4) }
}

pub fn run() {
    probe();
    for n in [32usize, 48] {
        sweep_bench(n);
    }
}

fn probe() {
    let src = [V4([0, 1, 2, 3]), V4([4, 5, 6, 7])];
    let mut out = [V4([0; 4]); 2];
    let shift_in = V4([-8, 8, 100, -100]);
    let mut shift_out = V4([0; 4]);
    unsafe {
        core::arch::asm!(
            "ee.vld.128.ip q0, {a}, 0",
            "ee.vld.128.ip q1, {b}, 0",
            "movi {t}, 4",
            "wur.sar_byte {t}",
            "ee.src.q q2, q0, q1",
            "ee.vst.128.ip q2, {o0}, 0",
            "movi {t}, 12",
            "wur.sar_byte {t}",
            "ee.src.q q2, q0, q1",
            "ee.vst.128.ip q2, {o1}, 0",
            "ee.vld.128.ip q3, {si}, 0",
            "ssai 2",
            "ee.vsr.32 q4, q3",
            "ee.vst.128.ip q4, {so}, 0",
            a = in(reg) src[0].0.as_ptr(),
            b = in(reg) src[1].0.as_ptr(),
            o0 = in(reg) out[0].0.as_mut_ptr(),
            o1 = in(reg) out[1].0.as_mut_ptr(),
            si = in(reg) shift_in.0.as_ptr(),
            so = in(reg) shift_out.0.as_mut_ptr(),
            t = out(reg) _,
        );
    }
    esp_println::println!(
        "[fluid-pie] probe src.q(q0=0..3,q1=4..7) sar_byte=4 -> {:?}, 12 -> {:?}; vsr.32 [-8,8,100,-100]>>2 -> {:?}",
        out[0].0,
        out[1].0,
        shift_out.0
    );
}

fn sweep_bench(n: usize) {
    let s = (n + 2 + 3) & !3; // row stride, whole vectors
    let rows = n + 2;
    let nv = rows * s / 4;
    let mut div = vec![V4([0; 4]); nv];
    let mut seed: u32 = 0x1234_5678;
    for (k, c) in cells(&mut div).iter_mut().enumerate() {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let (i, j) = (k % s, k / s);
        *c = if (1..=n).contains(&i) && (1..=n).contains(&j) {
            ((seed >> 16) as i32 & 0x1fff) - 0x1000
        } else {
            0
        };
    }
    let iters = 10;
    let time = |f: &mut dyn FnMut()| {
        let t0 = cycle_counter::read();
        f();
        let dt = cycle_counter::read().wrapping_sub(t0) as u64;
        dt * 1_000_000 / CPU_HZ
    };

    // Today's ordering: Gauss-Seidel in place, scalar Q16.16 (wrapping).
    let mut gs = vec![V4([0; 4]); nv];
    let us_gs = time(&mut || {
        let (x, d) = (cells(&mut gs).as_mut_ptr(), cells(&mut div).as_ptr());
        for _ in 0..iters {
            for j in 1..=n {
                for i in 1..=n {
                    let c = i + s * j;
                    unsafe {
                        let sum = (*x.add(c - 1))
                            .wrapping_add(*x.add(c + 1))
                            .wrapping_add(*x.add(c - s))
                            .wrapping_add(*x.add(c + s))
                            .wrapping_add(*d.add(c));
                        *x.add(c) = sum >> 2;
                    }
                }
            }
            bnd(cells(&mut gs), n, s);
        }
    });

    // Jacobi, scalar.
    let mut ja = vec![V4([0; 4]); nv];
    let mut jb = vec![V4([0; 4]); nv];
    let us_jac = time(&mut || {
        for _ in 0..iters {
            {
                let (x, y, d) = (cells(&mut ja).as_ptr(), cells(&mut jb).as_mut_ptr(), cells(&mut div).as_ptr());
                for j in 1..=n {
                    for i in 1..=n {
                        let c = i + s * j;
                        unsafe {
                            let sum = (*x.add(c - 1))
                                .wrapping_add(*x.add(c + 1))
                                .wrapping_add(*x.add(c - s))
                                .wrapping_add(*x.add(c + s))
                                .wrapping_add(*d.add(c));
                            *y.add(c) = sum >> 2;
                        }
                    }
                }
            }
            bnd(cells(&mut jb), n, s);
            core::mem::swap(&mut ja, &mut jb);
        }
    });

    // Jacobi, scalar f32.
    let divf: Vec<f32> = cells(&mut div).iter().map(|&v| v as f32 / 65536.0).collect();
    let mut fa = vec![0f32; rows * s];
    let mut fb = vec![0f32; rows * s];
    let us_f32 = time(&mut || {
        for _ in 0..iters {
            for j in 1..=n {
                for i in 1..=n {
                    let c = i + s * j;
                    fb[c] = (fa[c - 1] + fa[c + 1] + fa[c - s] + fa[c + s] + divf[c]) * 0.25;
                }
            }
            bnd_f(&mut fb, n, s);
            core::mem::swap(&mut fa, &mut fb);
        }
    });

    // Jacobi, PIE: a whole row a call, four cells a vector. Rows include the
    // ghost columns and the padding (their lanes compute garbage that the
    // boundary pass overwrites), so every load is a whole aligned vector.
    let mut pa = vec![V4([0; 4]); nv];
    let mut pb = vec![V4([0; 4]); nv];
    let vecs = s / 4;
    let us_pie = time(&mut || {
        for _ in 0..iters {
            {
                let x = pa.as_ptr();
                let y = pb.as_mut_ptr();
                let d = div.as_ptr();
                for j in 1..=n {
                    let row = j * vecs;
                    unsafe { pie_row(x.add(row), d.add(row), y.add(row), vecs, s * 4) };
                }
            }
            bnd(cells(&mut pb), n, s);
            core::mem::swap(&mut pa, &mut pb);
        }
    });

    let same = {
        let (a, b) = (cells(&mut ja), cells(&mut pa));
        (1..=n).all(|j| (1..=n).all(|i| a[i + s * j] == b[i + s * j]))
    };
    let per = |us: u64| us * 1000 / iters as u64;
    esp_println::println!(
        "[fluid-pie] n={n} stride={s} ns/sweep: gs-q16={} jacobi-q16={} jacobi-f32={} jacobi-pie={} pie==jacobi-q16: {}",
        per(us_gs),
        per(us_jac),
        per(us_f32),
        per(us_pie),
        same
    );
}

/// One row of the Jacobi sweep in PIE. `x`, `d`, `y` point at the row's
/// first vector; `row_bytes` is the stride in bytes (a multiple of 16).
#[inline(never)]
unsafe fn pie_row(x: *const V4, d: *const V4, y: *mut V4, vecs: usize, row_bytes: usize) {
    unsafe {
        core::arch::asm!(
            // q0 = the vector left of the row, q1 = the row's first.
            "addi {l}, {c}, -16",
            "ee.vld.128.ip q0, {l}, 0",
            "ee.vld.128.ip q1, {c}, 16",
            "sub {u}, {c}, {rb}",
            "addi {u}, {u}, -16",
            "add {dn}, {c}, {rb}",
            "addi {dn}, {dn}, -16",
            "ssai 2",
            "loop {n}, 2f",
            "ee.vld.128.ip q2, {c}, 16",
            "movi {t}, 12",
            "wur.sar_byte {t}",
            "ee.src.q q3, q0, q1",
            "movi {t}, 4",
            "wur.sar_byte {t}",
            "ee.src.q q4, q1, q2",
            "ee.vld.128.ip q5, {u}, 16",
            "ee.vld.128.ip q6, {dn}, 16",
            "ee.vadds.s32 q3, q3, q4",
            "ee.vld.128.ip q7, {d}, 16",
            "ee.vadds.s32 q5, q5, q6",
            "ee.vadds.s32 q3, q3, q5",
            "ee.vadds.s32 q3, q3, q7",
            "ee.vsr.32 q3, q3",
            "ee.vst.128.ip q3, {y}, 16",
            "mv.qr q0, q1",
            "mv.qr q1, q2",
            "2:",
            c = inout(reg) x => _,
            d = inout(reg) d => _,
            y = inout(reg) y => _,
            n = in(reg) vecs,
            rb = in(reg) row_bytes,
            l = out(reg) _,
            u = out(reg) _,
            dn = out(reg) _,
            t = out(reg) _,
        );
    }
}

fn bnd(x: &mut [i32], n: usize, s: usize) {
    for k in 1..=n {
        x[s * k] = x[1 + s * k];
        x[n + 1 + s * k] = x[n + s * k];
        x[k] = x[k + s];
        x[k + s * (n + 1)] = x[k + s * n];
    }
}

fn bnd_f(x: &mut [f32], n: usize, s: usize) {
    for k in 1..=n {
        x[s * k] = x[1 + s * k];
        x[n + 1 + s * k] = x[n + s * k];
        x[k] = x[k + s];
        x[k + s * (n + 1)] = x[k + s * n];
    }
}
