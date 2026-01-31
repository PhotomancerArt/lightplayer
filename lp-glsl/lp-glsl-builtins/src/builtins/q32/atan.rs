//! Fixed-point 16.16 arctangent function.

use super::atan2::__lp_q32_atan2;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536

/// Compute atan(x) using atan2: atan(x) = atan2(x, 1)
///
/// Algorithm ported from libfixmath.
/// Returns angle in radians in range [-π/2, π/2].
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_atan(x: i32) -> i32 {
    __lp_q32_atan2(x, FIX16_ONE)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::test_q32_function_relative;

    #[test]
    fn test_atan_basic() {
        let tests = [
            (0.0, 0.0),
            (1.0, 0.7853981633974483),                // atan(1) = π/4
            (-1.0, -0.7853981633974483),              // atan(-1) = -π/4
            (0.5773502691896257, 0.5235987755982988), // atan(√3/3) = π/6
        ];

        // Use 3% tolerance for trig functions
        test_q32_function_relative(|x| __lp_q32_atan(x), &tests, 0.03, 0.01);
    }
}
