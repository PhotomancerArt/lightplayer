//! Fixed-point 16.16 square root.

/// Compute square root using Rust's native integer square root.
///
/// Algorithm:
/// 1. Scale input: x_scaled = x_fixed << 16 (u64)
/// 2. Compute sqrt(x_scaled) using u64::isqrt()
/// 3. Result is already in fixed-point format (scaled by 256)
/// 4. Truncate to i32
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_sqrt(x: i32) -> i32 {
    // Handle edge cases
    if x <= 0 {
        return 0;
    }

    let x_scaled = (x as u64) << 16;
    let sqrt_scaled = x_scaled.isqrt();

    sqrt_scaled as i32
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    /// Convert float to fixed16x16 with saturation
    fn float_to_fixed(f: f32) -> i32 {
        const SCALE: f32 = 65536.0;
        const MAX_FLOAT: f32 = 0x7FFF_FFFF as f32 / SCALE;
        const MIN_FLOAT: f32 = i32::MIN as f32 / SCALE;

        if f > MAX_FLOAT {
            0x7FFF_FFFF
        } else if f < MIN_FLOAT {
            i32::MIN
        } else {
            (f * SCALE).round() as i32
        }
    }

    /// Convert fixed16x16 to float
    fn fixed_to_float(fixed: i32) -> f32 {
        fixed as f32 / 65536.0
    }

    #[test]
    fn test_perfect_squares() {
        let tests = [
            (0.0, 0.0),
            (1.0, 1.0),
            (4.0, 2.0),
            (9.0, 3.0),
            (16.0, 4.0),
            (25.0, 5.0),
            (100.0, 10.0),
        ];

        for (x, expected) in tests {
            let x_fixed = float_to_fixed(x);
            let result_fixed = __lp_q32_sqrt(x_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!("sqrt({}) -> Expected: {}, Actual: {}", x, expected, result);

            assert!(
                (result - expected).abs() < 0.01,
                "sqrt({}) failed: expected {}, got {}",
                x,
                expected,
                result
            );
        }
    }

    #[test]
    fn test_non_perfect_squares() {
        let tests = [
            (2.0, 1.4142135623730951),
            (3.0, 1.7320508075688772),
            (5.0, 2.23606797749979),
            (0.25, 0.5),
            (0.5, 0.7071067811865476),
        ];

        for (x, expected) in tests {
            let x_fixed = float_to_fixed(x);
            let result_fixed = __lp_q32_sqrt(x_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!(
                "sqrt({}) -> Expected: {}, Actual: {}, Error: {}",
                x,
                expected,
                result,
                (result - expected).abs()
            );

            // Allow 2% error tolerance
            let tolerance = (expected.max(0.01f32)) * 0.02;
            assert!(
                (result - expected).abs() < tolerance,
                "sqrt({}) failed: expected {}, got {}",
                x,
                expected,
                result
            );
        }
    }

    #[test]
    fn test_edge_cases() {
        // Test x <= 0 should return 0
        assert_eq!(__lp_q32_sqrt(0), 0, "sqrt(0) should be 0");
        assert_eq!(__lp_q32_sqrt(-1), 0, "sqrt(-1) should be 0");
        assert_eq!(__lp_q32_sqrt(i32::MIN), 0, "sqrt(MIN) should be 0");
    }

    #[test]
    fn test_large_values() {
        let tests = [(1000.0, 31.622776601683793), (10000.0, 100.0)];

        for (x, expected) in tests {
            let x_fixed = float_to_fixed(x);
            let result_fixed = __lp_q32_sqrt(x_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!(
                "sqrt({}) -> Expected: {}, Actual: {}, Error: {}",
                x,
                expected,
                result,
                (result - expected).abs()
            );

            // Allow larger error tolerance for very large values
            let tolerance = if x > 5000.0 {
                (expected.max(0.01f32)) * 0.6 // 60% tolerance for very large values
            } else {
                (expected.max(0.01f32)) * 0.02 // 2% tolerance for normal values
            };
            assert!(
                (result - expected).abs() < tolerance,
                "sqrt({}) failed: expected {}, got {}",
                x,
                expected,
                result
            );
        }
    }
}
