// test run
// target riscv32.q32

// ============================================================================
// From Scalar: vec4(float) - broadcast single float to all components
// ============================================================================

vec4 test_vec4_from_scalar_positive() {
    // Constructor vec4(float) broadcasts single float to all components
    return vec4(5.0);
}

// run: test_vec4_from_scalar_positive() ~= vec4(5.0, 5.0, 5.0, 5.0)

vec4 test_vec4_from_scalar_negative() {
    return vec4(-3.0);
}

// run: test_vec4_from_scalar_negative() ~= vec4(-3.0, -3.0, -3.0, -3.0)

vec4 test_vec4_from_scalar_zero() {
    return vec4(0.0);
}

// run: test_vec4_from_scalar_zero() ~= vec4(0.0, 0.0, 0.0, 0.0)

vec4 test_vec4_from_scalar_variable() {
    float x = 42.0;
    return vec4(x);
}

// run: test_vec4_from_scalar_variable() ~= vec4(42.0, 42.0, 42.0, 42.0)

vec4 test_vec4_from_scalar_expression() {
    return vec4(10.0 - 5.0);
}

// run: test_vec4_from_scalar_expression() ~= vec4(5.0, 5.0, 5.0, 5.0)

vec4 test_vec4_from_scalar_function_result() {
    return vec4(float(7)); // int to float conversion
}

// run: test_vec4_from_scalar_function_result() ~= vec4(7.0, 7.0, 7.0, 7.0)

vec4 test_vec4_from_scalar_in_assignment() {
    vec4 result;
    result = vec4(-8.0);
    return result;
}

// run: test_vec4_from_scalar_in_assignment() ~= vec4(-8.0, -8.0, -8.0, -8.0)

vec4 test_vec4_from_scalar_large_value() {
    // Large values are clamped to fixed16x16 max (32767.99998)
    return vec4(100000.0);
}

// run: test_vec4_from_scalar_large_value() ~= vec4(32768.0, 32768.0, 32768.0, 32768.0)

vec4 test_vec4_from_scalar_fractional() {
    return vec4(0.5);
}

// run: test_vec4_from_scalar_fractional() ~= vec4(0.5, 0.5, 0.5, 0.5)

vec4 test_vec4_from_scalar_computation() {
    return vec4(2.0 * 3.0 + 1.0);
}

// run: test_vec4_from_scalar_computation() ~= vec4(7.0, 7.0, 7.0, 7.0)
