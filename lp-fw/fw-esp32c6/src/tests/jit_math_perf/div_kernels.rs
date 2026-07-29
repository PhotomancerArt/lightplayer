//! Q32 reciprocal division baselines and candidate kernels.

use log::info;

use super::corpus::{DIVIDENDS, DIVISORS, Q_ONE, volatile_i32};
use super::mul_kernels::wrapping_i64_mul;
use super::runner;

use lps_builtins::builtins::lpir::fdiv_recip_q32::__lp_lpir_fdiv_recip_q32;

const MAX_FIXED: i32 = 0x7FFF_FFFF;
const MIN_FIXED: i32 = i32::MIN;

pub fn run() {
    info!("[jit-math-perf] --- division kernels ---");
    let calls = DIVIDENDS.len() * DIVISORS.len();
    runner::measure("div/helper-recip", calls, || sweep_div(helper_div));
    runner::measure("div/inline-recip-rust", calls, || {
        sweep_div(inline_recip_div)
    });

    const_div_bench("div/const-2", 2 * Q_ONE);
    const_div_bench("div/const-3", 3 * Q_ONE);
    const_div_bench("div/const-6", 6 * Q_ONE);
    const_div_bench("div/const-255", 255 * Q_ONE);
    runner::measure("div/pow2-shift-2", DIVIDENDS.len(), || {
        sweep_unary_div(|v| div_by_positive_pow2(v, 2 * Q_ONE))
    });
}

fn sweep_div(kernel: fn(i32, i32) -> i32) -> i32 {
    let mut acc = 0i32;
    for i in 0..DIVIDENDS.len() {
        let dividend = volatile_i32(&DIVIDENDS, i);
        for j in 0..DIVISORS.len() {
            let divisor = volatile_i32(&DIVISORS, j);
            acc = acc.wrapping_add(kernel(dividend, divisor));
        }
    }
    acc
}

fn const_div_bench(label: &str, divisor: i32) {
    let recip2 = precompute_recip2(divisor);
    runner::measure(label, DIVIDENDS.len(), || {
        sweep_unary_div(|v| const_recip_div(v, divisor, recip2))
    });
}

fn sweep_unary_div<F>(mut kernel: F) -> i32
where
    F: FnMut(i32) -> i32,
{
    let mut acc = 0i32;
    for i in 0..DIVIDENDS.len() {
        acc = acc.wrapping_add(kernel(volatile_i32(&DIVIDENDS, i)));
    }
    acc
}

#[inline(never)]
fn helper_div(dividend: i32, divisor: i32) -> i32 {
    __lp_lpir_fdiv_recip_q32(dividend, divisor)
}

#[inline(never)]
fn inline_recip_div(dividend: i32, divisor: i32) -> i32 {
    if divisor == 0 {
        return div_by_zero(dividend);
    }

    let result_sign = if (dividend ^ divisor) < 0 {
        -1i32
    } else {
        1i32
    };
    let abs_dividend = dividend.unsigned_abs();
    let abs_divisor = divisor.unsigned_abs();
    let recip = 0x8000_0000u32 / abs_divisor;
    let quot = (((abs_dividend as u64) * (recip as u64) * 2u64) >> 16) as u32;
    (quot as i32).wrapping_mul(result_sign)
}

#[inline(never)]
fn const_recip_div(dividend: i32, divisor: i32, recip2: u32) -> i32 {
    if divisor == 0 {
        return div_by_zero(dividend);
    }
    let sign = if (dividend ^ divisor) < 0 { -1 } else { 1 };
    let quot = (((dividend.unsigned_abs() as u64) * (recip2 as u64)) >> 16) as u32;
    (quot as i32).wrapping_mul(sign)
}

#[inline(never)]
fn div_by_positive_pow2(dividend: i32, divisor: i32) -> i32 {
    debug_assert!(divisor > 0);
    if divisor == Q_ONE {
        return dividend;
    }
    if divisor == 2 * Q_ONE {
        return dividend >> 1;
    }
    inline_recip_div(dividend, divisor)
}

fn precompute_recip2(divisor: i32) -> u32 {
    if divisor == 0 {
        0
    } else {
        (0x8000_0000u32 / divisor.unsigned_abs()).wrapping_mul(2)
    }
}

fn div_by_zero(dividend: i32) -> i32 {
    if dividend == 0 {
        0
    } else if dividend > 0 {
        MAX_FIXED
    } else {
        MIN_FIXED
    }
}

#[allow(
    dead_code,
    reason = "kept as a sanity reference for const-divisor experiments"
)]
fn div_by_const_as_mul(dividend: i32, divisor_recip_q32: i32) -> i32 {
    wrapping_i64_mul(dividend, divisor_recip_q32)
}
