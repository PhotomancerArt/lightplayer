// test run
// target riscv32.q32

// ============================================================================
// From Scalar: vec2(float) - broadcast single float to all components
// ============================================================================

vec2 test_vec2_from_scalar_positive() {
    // Constructor vec2(float) broadcasts single float to all components
    return vec2(5.0);
}

// run: test_vec2_from_scalar_positive() ~= vec2(5.0, 5.0)

vec2 test_vec2_from_scalar_negative() {
    return vec2(-3.0);
}

// run: test_vec2_from_scalar_negative() ~= vec2(-3.0, -3.0)

vec2 test_vec2_from_scalar_zero() {
    return vec2(0.0);
}

// run: test_vec2_from_scalar_zero() ~= vec2(0.0, 0.0)

vec2 test_vec2_from_scalar_variable() {
    float x = 42.0;
    return vec2(x);
}

// run: test_vec2_from_scalar_variable() ~= vec2(42.0, 42.0)

vec2 test_vec2_from_scalar_expression() {
    return vec2(10.0 - 5.0);
}

// run: test_vec2_from_scalar_expression() ~= vec2(5.0, 5.0)

vec2 test_vec2_from_scalar_function_result() {
    return vec2(float(7)); // int to float conversion
}

// run: test_vec2_from_scalar_function_result() ~= vec2(7.0, 7.0)

vec2 test_vec2_from_scalar_in_assignment() {
    vec2 result;
    result = vec2(-8.0);
    return result;
}

// run: test_vec2_from_scalar_in_assignment() ~= vec2(-8.0, -8.0)

vec2 test_vec2_from_scalar_large_value() {
    // Large values are clamped to fixed16x16 max (32767.99998)
    return vec2(100000.0);
}

// run: test_vec2_from_scalar_large_value() ~= vec2(32768.0, 32768.0)

vec2 test_vec2_from_scalar_fractional() {
    return vec2(0.5);
}

// run: test_vec2_from_scalar_fractional() ~= vec2(0.5, 0.5)

vec2 test_vec2_from_scalar_computation() {
    return vec2(2.0 * 3.0 + 1.0);
}

// run: test_vec2_from_scalar_computation() ~= vec2(7.0, 7.0)
