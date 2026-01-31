// test run
// target riscv32.q32

// ============================================================================
// From Scalar: vec3(float) - broadcast single float to all components
// ============================================================================

vec3 test_vec3_from_scalar_positive() {
    // Constructor vec3(float) broadcasts single float to all components
    return vec3(5.0);
}

// run: test_vec3_from_scalar_positive() ~= vec3(5.0, 5.0, 5.0)

vec3 test_vec3_from_scalar_negative() {
    return vec3(-3.0);
}

// run: test_vec3_from_scalar_negative() ~= vec3(-3.0, -3.0, -3.0)

vec3 test_vec3_from_scalar_zero() {
    return vec3(0.0);
}

// run: test_vec3_from_scalar_zero() ~= vec3(0.0, 0.0, 0.0)

vec3 test_vec3_from_scalar_variable() {
    float x = 42.0;
    return vec3(x);
}

// run: test_vec3_from_scalar_variable() ~= vec3(42.0, 42.0, 42.0)

vec3 test_vec3_from_scalar_expression() {
    return vec3(10.0 - 5.0);
}

// run: test_vec3_from_scalar_expression() ~= vec3(5.0, 5.0, 5.0)

vec3 test_vec3_from_scalar_function_result() {
    return vec3(float(7)); // int to float conversion
}

// run: test_vec3_from_scalar_function_result() ~= vec3(7.0, 7.0, 7.0)

vec3 test_vec3_from_scalar_in_assignment() {
    vec3 result;
    result = vec3(-8.0);
    return result;
}

// run: test_vec3_from_scalar_in_assignment() ~= vec3(-8.0, -8.0, -8.0)

vec3 test_vec3_from_scalar_large_value() {
    // Large values are clamped to fixed16x16 max (32767.99998)
    return vec3(100000.0);
}

// run: test_vec3_from_scalar_large_value() ~= vec3(32768.0, 32768.0, 32768.0)

vec3 test_vec3_from_scalar_fractional() {
    return vec3(0.5);
}

// run: test_vec3_from_scalar_fractional() ~= vec3(0.5, 0.5, 0.5)

vec3 test_vec3_from_scalar_computation() {
    return vec3(2.0 * 3.0 + 1.0);
}

// run: test_vec3_from_scalar_computation() ~= vec3(7.0, 7.0, 7.0)
