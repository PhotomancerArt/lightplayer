//! Test helper functions for q32 math functions.

#[cfg(test)]
extern crate std;

/// Convert float to fixed16x16 with saturation
pub fn float_to_fixed(f: f32) -> i32 {
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
pub fn fixed_to_float(fixed: i32) -> f32 {
    fixed as f32 / 65536.0
}

/// Test a q32 function with a list of (input, expected_output) pairs.
///
/// # Arguments
/// * `func` - The function to test (takes i32, returns i32)
/// * `test_cases` - Array of (input_float, expected_output_float) pairs
/// * `tolerance` - Maximum allowed absolute error between expected and actual
#[allow(dead_code, reason = "Test helper function")]
pub fn test_q32_function<F>(func: F, test_cases: &[(f32, f32)], tolerance: f32)
where
    F: Fn(i32) -> i32 + Copy,
{
    for (input_float, expected_float) in test_cases {
        let input_fixed = float_to_fixed(*input_float);
        let result_fixed = func(input_fixed);
        let result_float = fixed_to_float(result_fixed);

        std::println!(
            "Test: f({}) -> Expected: {}, Actual: {}, Error: {}",
            input_float,
            expected_float,
            result_float,
            (result_float - expected_float).abs()
        );

        assert!(
            (result_float - expected_float).abs() < tolerance,
            "Test failed: f({}); expected {}, got {} (error: {})",
            input_float,
            expected_float,
            result_float,
            (result_float - expected_float).abs()
        );
    }
}

/// Test a q32 function with relative tolerance.
///
/// Uses relative tolerance: `|actual - expected| < max(expected.abs() * tolerance, min_tolerance)`
///
/// # Arguments
/// * `func` - The function to test (takes i32, returns i32)
/// * `test_cases` - Array of (input_float, expected_output_float) pairs
/// * `tolerance` - Relative tolerance (e.g., 0.02 for 2%)
/// * `min_tolerance` - Minimum absolute tolerance for values near zero
pub fn test_q32_function_relative<F>(
    func: F,
    test_cases: &[(f32, f32)],
    tolerance: f32,
    min_tolerance: f32,
) where
    F: Fn(i32) -> i32 + Copy,
{
    for (input_float, expected_float) in test_cases {
        let input_fixed = float_to_fixed(*input_float);
        let result_fixed = func(input_fixed);
        let result_float = fixed_to_float(result_fixed);

        let abs_error = (result_float - expected_float).abs();
        let rel_tolerance = expected_float.abs() * tolerance;
        let effective_tolerance = rel_tolerance.max(min_tolerance);

        std::println!(
            "Test: f({}) -> Expected: {}, Actual: {}, Error: {}, Tolerance: {}",
            input_float,
            expected_float,
            result_float,
            abs_error,
            effective_tolerance
        );

        assert!(
            abs_error < effective_tolerance,
            "Test failed: f({}); expected {}, got {} (error: {}, tolerance: {})",
            input_float,
            expected_float,
            result_float,
            abs_error,
            effective_tolerance
        );
    }
}
