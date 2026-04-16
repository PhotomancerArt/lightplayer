//! Q16.16 operations matching LPIR builtin semantics.
//!
//! These are `pub(crate)` without `#[no_mangle]` so we do not duplicate linker
//! symbols from `lps-builtins` (`__lp_lpir_*` / `__lps_*`).

const MAX_FIXED: i32 = 0x7FFF_FFFF;
const MIN_FIXED: i32 = i32::MIN;

/// Fixed-point value of π (Q16.16 format)
const FIX16_PI: i32 = 205887;

#[inline(always)]
pub(crate) fn fmul_q32(a: i32, b: i32) -> i32 {
    if a == 0 || b == 0 {
        return 0;
    }

    let a_wide = a as i64;
    let b_wide = b as i64;
    let mul_result_wide = a_wide * b_wide;
    let shifted_wide = mul_result_wide >> 16;

    if shifted_wide > MAX_FIXED as i64 {
        MAX_FIXED
    } else if shifted_wide < MIN_FIXED as i64 {
        MIN_FIXED
    } else {
        shifted_wide as i32
    }
}

#[inline(always)]
pub(crate) fn fdiv_q32(dividend: i32, divisor: i32) -> i32 {
    if divisor == 0 {
        if dividend == 0 {
            0
        } else if dividend > 0 {
            MAX_FIXED
        } else {
            MIN_FIXED
        }
    } else {
        let dividend_wide = dividend as i64;
        let divisor_wide = divisor as i64;
        let result_wide = (dividend_wide << 16) / divisor_wide;

        if result_wide > MAX_FIXED as i64 {
            MAX_FIXED
        } else if result_wide < MIN_FIXED as i64 {
            MIN_FIXED
        } else {
            result_wide as i32
        }
    }
}

#[inline(always)]
pub(crate) fn fsqrt_q32(x: i32) -> i32 {
    if x <= 0 {
        0
    } else {
        let x_scaled = (x as u64) << 16;
        let sqrt_scaled = x_scaled.isqrt();
        sqrt_scaled as i32
    }
}

/// Sine (Taylor + range reduction), aligned with `__lps_sin_q32`.
#[inline]
pub(crate) fn sin_q32(x: i32) -> i32 {
    if x == 0 {
        return 0;
    }

    let two_pi = FIX16_PI << 1;
    let mut temp_angle = x % two_pi;

    if temp_angle > FIX16_PI {
        temp_angle -= two_pi;
    } else if temp_angle < -FIX16_PI {
        temp_angle += two_pi;
    }

    let temp_angle_sq = fmul_q32(temp_angle, temp_angle);
    let mut result = temp_angle;

    let mut term = fmul_q32(temp_angle, temp_angle_sq);
    result -= term / 6;

    term = fmul_q32(term, temp_angle_sq);
    result += term / 120;

    term = fmul_q32(term, temp_angle_sq);
    result -= term / 5040;

    term = fmul_q32(term, temp_angle_sq);
    result += term / 362880;

    term = fmul_q32(term, temp_angle_sq);
    result -= term / 39916800;

    result
}

#[inline(always)]
pub(crate) fn cos_q32(x: i32) -> i32 {
    let half_pi = FIX16_PI >> 1;
    sin_q32(x.wrapping_add(half_pi))
}

#[inline(always)]
pub(crate) fn mod_q32(x: i32, y: i32) -> i32 {
    let div_result = fdiv_q32(x, y);
    let floored = (div_result >> 16) << 16;
    let y_times_floor = fmul_q32(y, floored);
    x.wrapping_sub(y_times_floor)
}
