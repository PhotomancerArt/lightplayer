//! Q32 trig baselines and candidate kernels.

extern crate alloc;

use alloc::vec::Vec;

use log::info;

use super::corpus::{ANGLES, Q_FRAC_PI_2, Q_ONE, Q_PI, Q_TAU, volatile_i32};
use super::mul_kernels::wrapping_i64_mul;
use super::runner;

use lps_builtins::builtins::glsl::sin_q32::__lps_sin_q32;
use lps_builtins::builtins::lpir::fmul_q32::__lp_lpir_fmul_q32;

const FAST_B: i32 = 83_443; // 4 / pi
const FAST_C: i32 = -26_561; // -4 / pi^2
const FAST_P: i32 = 14_746; // 0.225

pub fn run() {
    info!("[jit-math-perf] --- trig kernels ---");
    runner::measure("trig/sin-reference-taylor", ANGLES.len(), || {
        sweep_sin(reference_taylor_sin)
    });
    runner::measure("trig/sin-current-builtin", ANGLES.len(), || {
        sweep_sin(current_sin)
    });
    runner::measure("trig/sin-fast-parabolic", ANGLES.len(), || {
        sweep_sin(fast_parabolic_sin)
    });
    runner::measure("trig/sin-cubic", ANGLES.len(), || sweep_sin(cubic_sin));

    quality("sin-fast-parabolic", fast_parabolic_sin);
    quality("sin-cubic", cubic_sin);

    lut_suite(256);
    lut_suite(512);
    lut_suite(1024);
    lut_suite(2048);
}

fn sweep_sin(kernel: fn(i32) -> i32) -> i32 {
    let mut acc = 0i32;
    for i in 0..ANGLES.len() {
        acc = acc.wrapping_add(kernel(volatile_i32(&ANGLES, i)));
    }
    acc
}

fn lut_suite(size: usize) {
    let table = build_sine_lut(size);
    let calls = ANGLES.len();
    runner::measure(&alloc::format!("trig/lut-nearest-{size}"), calls, || {
        sweep_lut(&table, false)
    });
    runner::measure(&alloc::format!("trig/lut-linear-{size}"), calls, || {
        sweep_lut(&table, true)
    });
    quality_lut(&alloc::format!("sin-lut-nearest-{size}"), &table, false);
    quality_lut(&alloc::format!("sin-lut-linear-{size}"), &table, true);
}

fn sweep_lut(table: &[i16], linear: bool) -> i32 {
    let mut acc = 0i32;
    for i in 0..ANGLES.len() {
        let angle = volatile_i32(&ANGLES, i);
        let value = if linear {
            lut_linear_sin(angle, table)
        } else {
            lut_nearest_sin(angle, table)
        };
        acc = acc.wrapping_add(value);
    }
    acc
}

fn quality(label: &str, kernel: fn(i32) -> i32) {
    let mut max_abs = 0i32;
    let mut sum_abs = 0u64;
    let mut worst = 0i32;
    for &angle in &ANGLES {
        let expected = reference_taylor_sin(angle);
        let got = kernel(angle);
        let err = expected.wrapping_sub(got).abs();
        if err > max_abs {
            max_abs = err;
            worst = angle;
        }
        sum_abs += err as u64;
    }
    let mean_abs = sum_abs / ANGLES.len() as u64;
    info!(
        "[jit-math-perf] quality {label:<24} max_abs={max_abs:>8} mean_abs={mean_abs:>8} \
         worst_angle={worst}",
    );
}

fn quality_lut(label: &str, table: &[i16], linear: bool) {
    let mut max_abs = 0i32;
    let mut sum_abs = 0u64;
    let mut worst = 0i32;
    for &angle in &ANGLES {
        let expected = reference_taylor_sin(angle);
        let got = if linear {
            lut_linear_sin(angle, table)
        } else {
            lut_nearest_sin(angle, table)
        };
        let err = expected.wrapping_sub(got).abs();
        if err > max_abs {
            max_abs = err;
            worst = angle;
        }
        sum_abs += err as u64;
    }
    let mean_abs = sum_abs / ANGLES.len() as u64;
    info!(
        "[jit-math-perf] quality {label:<24} max_abs={max_abs:>8} mean_abs={mean_abs:>8} \
         worst_angle={worst}",
    );
}

#[inline(never)]
fn current_sin(angle: i32) -> i32 {
    __lps_sin_q32(angle)
}

#[inline(never)]
fn reference_taylor_sin(angle: i32) -> i32 {
    if angle == 0 {
        return 0;
    }

    let x = fold_pi(angle);
    let x2 = __lp_lpir_fmul_q32(x, x);
    let mut result = x;
    let mut term = __lp_lpir_fmul_q32(x, x2);
    result -= term / 6;
    term = __lp_lpir_fmul_q32(term, x2);
    result += term / 120;
    term = __lp_lpir_fmul_q32(term, x2);
    result -= term / 5040;
    term = __lp_lpir_fmul_q32(term, x2);
    result += term / 362880;
    term = __lp_lpir_fmul_q32(term, x2);
    result -= term / 39916800;
    result
}

#[inline(never)]
fn fast_parabolic_sin(angle: i32) -> i32 {
    let x = fold_pi(angle);
    let ax = x.wrapping_abs();
    let y =
        wrapping_i64_mul(FAST_B, x).wrapping_add(wrapping_i64_mul(FAST_C, wrapping_i64_mul(x, ax)));
    let ay = y.wrapping_abs();
    qmul_trunc_zero(FAST_P, wrapping_i64_mul(y, ay).wrapping_sub(y)).wrapping_add(y)
}

#[inline(never)]
fn cubic_sin(angle: i32) -> i32 {
    let x = fold_pi(angle);
    let x2 = wrapping_i64_mul(x, x);
    let correction = Q_ONE.wrapping_sub(x2 / 6);
    wrapping_i64_mul(x, correction)
}

fn build_sine_lut(size: usize) -> Vec<i16> {
    let mut table = Vec::with_capacity(size + 1);
    for i in 0..=size {
        let angle = ((i as i64 * Q_TAU as i64) / size as i64) as i32;
        let sample = (reference_taylor_sin(angle) >> 1).clamp(i16::MIN as i32, i16::MAX as i32);
        table.push(sample as i16);
    }
    table
}

#[inline(never)]
fn lut_nearest_sin(angle: i32, table: &[i16]) -> i32 {
    let size = table.len().saturating_sub(1);
    let phase = fold_tau_positive(angle) as u32;
    let index = ((phase as u64 * size as u64) / Q_TAU as u64) as usize;
    (table[index.min(size)] as i32) << 1
}

#[inline(never)]
fn lut_linear_sin(angle: i32, table: &[i16]) -> i32 {
    let size = table.len().saturating_sub(1);
    let phase = fold_tau_positive(angle) as u32;
    let scaled = phase as u64 * size as u64;
    let index = (scaled / Q_TAU as u64) as usize;
    let frac = ((scaled % Q_TAU as u64) * Q_ONE as u64 / Q_TAU as u64) as i32;
    let a = (table[index.min(size)] as i32) << 1;
    let b = (table[(index + 1).min(size)] as i32) << 1;
    a.wrapping_add(wrapping_i64_mul(b.wrapping_sub(a), frac))
}

fn fold_pi(mut angle: i32) -> i32 {
    angle %= Q_TAU;
    if angle > Q_PI {
        angle -= Q_TAU;
    } else if angle < -Q_PI {
        angle += Q_TAU;
    }
    angle
}

fn fold_tau_positive(mut angle: i32) -> i32 {
    angle %= Q_TAU;
    if angle < 0 {
        angle += Q_TAU;
    }
    angle
}

#[inline(always)]
fn qmul_trunc_zero(lhs: i32, rhs: i32) -> i32 {
    let product = lhs as i64 * rhs as i64;
    ((product + ((product >> 63) & 0xffff)) >> 16) as i32
}

#[allow(
    dead_code,
    reason = "paired sincos candidate may be used in a follow-up pass"
)]
fn fast_parabolic_cos(angle: i32) -> i32 {
    fast_parabolic_sin(angle.wrapping_add(Q_FRAC_PI_2))
}
