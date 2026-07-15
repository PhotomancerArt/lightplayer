// test run

// ============================================================================
// Divide: float / float -> float
// ============================================================================

float test_float_divide_positive_positive() {
    // Division with positive numbers
    return 10.0 / 2.0;
}

// run: test_float_divide_positive_positive() ~= 5.0

float test_float_divide_positive_negative() {
    return 10.0 / (-2.0);
}

// run: test_float_divide_positive_negative() ~= -5.0

float test_float_divide_negative_negative() {
    return (-10.0) / (-2.0);
}

// run: test_float_divide_negative_negative() ~= 5.0

float test_float_divide_by_one() {
    return 7.5 / 1.0;
}

// run: test_float_divide_by_one() ~= 7.5

float test_float_divide_variables() {
    float a = 15.0;
    float b = 3.0;
    return a / b;
}

// run: test_float_divide_variables() ~= 5.0 (tolerance: 0.001)

float test_float_divide_expressions() {
    return (20.0 / 2.0) / (4.0 / 2.0);
}

// run: test_float_divide_expressions() ~= 5.0

float test_float_divide_in_assignment() {
    float result = 10.0;
    result = result / 2.5;
    return result;
}

// run: test_float_divide_in_assignment() ~= 4.0

float test_float_divide_fractions() {
    return 0.5 / 0.25;
}

// run: test_float_divide_fractions() ~= 2.0

float test_float_divide_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998)
    // Fast reciprocal Q32 division is approximate near saturated dividends.
    // Use locals so Naga does not constant-fold before Q32 literal encoding.
    float a = 1000000.0;
    float b = 1000.0;
    return a / b;
}

// per-mode: the f32 channel asserts IEEE f32 results; Q32 keeps its saturation/wrapping expectation (M6 triage).
// @unsupported(rv32c.q32)
// run[q32]: test_float_divide_large_numbers() ~= 31.999985 (tolerance: 0.001)
// run[f32]: test_float_divide_large_numbers() ~= 1000.0 (tolerance: 0.001)
