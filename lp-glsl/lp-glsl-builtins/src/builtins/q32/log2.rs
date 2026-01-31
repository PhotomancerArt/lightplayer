//! Fixed-point 16.16 base-2 logarithm function.

use crate::builtins::q32::div::__lp_q32_div;
use crate::builtins::q32::mul;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536
/// Overflow value (returned for invalid inputs)
const FIX16_OVERFLOW: i32 = i32::MIN;

/// Right shift with rounding
#[inline]
fn fix16_rs(x: i32) -> i32 {
    (x >> 1) + (x & 1)
}

/// Inner log2 implementation for x >= 1
///
/// This assumes that the input value is >= 1.
/// Note that this is only ever called with inValue >= 1.
/// As such, the result is always less than the input.
///
/// libfixmath builds result as an integer, then shifts left by 16 at the end.
fn log2_inner(x: i32) -> i32 {
    // libfixmath builds result as integer, accumulating bits
    // result starts at 0, gets integer part added, then fractional bits accumulated
    // At the end, result is shifted left by 16 to convert to fixed point
    let mut result = 0i32;
    let mut x_val = x;

    // Count integer part: how many times we can divide by 2
    // libfixmath: while(x >= fix16_from_int(2)) { result++; x = fix16_rs(x); }
    // This adds the integer part directly to result (as integer, not fixed point)
    while x_val >= (2 << 16) {
        result += 1;
        x_val = fix16_rs(x_val);
    }

    // If x became 0, return integer part shifted to fixed point
    if x_val == 0 {
        return result << 16;
    }

    // Compute fractional part using binary search
    // Each iteration adds one bit of precision to the fractional part
    // libfixmath: for(i = 16; i > 0; i--) { x = fix16_mul(x, x); result <<= 1; ... }
    // After this loop, result has 16 fractional bits (bits 0-15)
    for _i in (1..=16).rev() {
        x_val = mul::__lp_q32_mul(x_val, x_val);
        result <<= 1; // Make room for next fractional bit
        if x_val >= (2 << 16) {
            result |= 1; // Set this fractional bit
            x_val = fix16_rs(x_val);
        }
    }

    // Final rounding step: check if we should round up
    x_val = mul::__lp_q32_mul(x_val, x_val);
    if x_val >= (2 << 16) {
        result += 1; // Round up
    }

    // libfixmath returns result directly without shifting
    // result has integer part and fractional bits already in the right positions
    result
}

/// Compute log2(x) using binary search method.
///
/// Algorithm ported from libfixmath.
/// For x < 1: log2(x) = -log2(1/x)
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_log2(x: i32) -> i32 {
    // Note that a negative x gives a non-real result.
    // If x == 0, the limit of log2(x) as x -> 0 = -infinity.
    // log2(-ve) gives a complex result.
    if x <= 0 {
        return FIX16_OVERFLOW;
    }

    // If the input is less than one, the result is -log2(1.0 / x)
    if x < FIX16_ONE {
        // Special case: log2(1/65536) = -16
        if x == 1 {
            return -(16 << 16);
        }

        let inverse = __lp_q32_div(FIX16_ONE, x);
        return -log2_inner(inverse);
    }

    // If input >= 1, just proceed as normal.
    // Note that x == fix16_one is a special case, where the answer is 0.
    if x == FIX16_ONE {
        return 0;
    }

    // For x = 2.0, log2_inner should return 1.0
    log2_inner(x)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_log2_basic() {
        let tests = [
            (1.0, 0.0),
            (2.0, 1.0),  // log2(2) = 1
            (4.0, 2.0),  // log2(4) = 2
            (0.5, -1.0), // log2(0.5) = -1
            (8.0, 3.0),  // log2(8) = 3
        ];

        // Use 3% tolerance for log2
        test_q32_function_relative(|x| __lp_q32_log2(x), &tests, 0.03, 0.01);
    }
}
