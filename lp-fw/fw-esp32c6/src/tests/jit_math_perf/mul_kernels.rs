//! Q32 multiply baselines and candidate kernels.

use log::info;

use super::corpus::{MUL_LHS, MUL_RHS, volatile_i32};
use super::runner;

use lps_builtins::builtins::lpir::fmul_q32::__lp_lpir_fmul_q32;

pub fn run() {
    info!("[jit-math-perf] --- multiply kernels ---");
    let calls = MUL_LHS.len() * MUL_RHS.len();
    runner::measure("mul/helper-saturating", calls, || sweep_mul(helper_mul));
    runner::measure("mul/wrapping-i64", calls, || sweep_mul(wrapping_i64_mul));
    runner::measure("mul/wrapping-parts", calls, || {
        sweep_mul(wrapping_parts_mul)
    });
}

fn sweep_mul(kernel: fn(i32, i32) -> i32) -> i32 {
    let mut acc = 0i32;
    for i in 0..MUL_LHS.len() {
        let lhs = volatile_i32(&MUL_LHS, i);
        for j in 0..MUL_RHS.len() {
            let rhs = volatile_i32(&MUL_RHS, j);
            acc = acc.wrapping_add(kernel(lhs, rhs));
        }
    }
    acc
}

#[inline(never)]
fn helper_mul(lhs: i32, rhs: i32) -> i32 {
    __lp_lpir_fmul_q32(lhs, rhs)
}

#[inline(never)]
pub fn wrapping_i64_mul(lhs: i32, rhs: i32) -> i32 {
    (((lhs as i64) * (rhs as i64)) >> 16) as i32
}

#[inline(never)]
fn wrapping_parts_mul(lhs: i32, rhs: i32) -> i32 {
    let product = (lhs as i64).wrapping_mul(rhs as i64) as u64;
    let lo = ((product as u32) >> 16) as i32;
    let hi = ((product >> 32) as u32).wrapping_shl(16) as i32;
    lo | hi
}
