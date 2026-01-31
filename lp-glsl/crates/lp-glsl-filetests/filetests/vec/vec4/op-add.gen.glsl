// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec4/op-add --write
//
// test run
// target riscv32.q32

// ============================================================================
// Add: vec4 + vec4 -> vec4 (component-wise)
// ============================================================================

vec4 test_vec4_add_positive_positive() {
    // Addition with positive vectors (component-wise)
    vec4 a = vec4(5.0, 3.0, 2.0, 1.0);
    vec4 b = vec4(2.0, 4.0, 1.0, 3.0);
    return a + b;
}

// run: test_vec4_add_positive_positive() ~= vec4(7.0, 7.0, 3.0, 4.0)

vec4 test_vec4_add_positive_negative() {
    vec4 a = vec4(10.0, 8.0, 5.0, 3.0);
    vec4 b = vec4(-4.0, -2.0, -1.0, -3.0);
    return a + b;
}

// run: test_vec4_add_positive_negative() ~= vec4(6.0, 6.0, 4.0, 0.0)

vec4 test_vec4_add_negative_negative() {
    vec4 a = vec4(-3.0, -7.0, -2.0, -5.0);
    vec4 b = vec4(-2.0, -1.0, -3.0, -1.0);
    return a + b;
}

// run: test_vec4_add_negative_negative() ~= vec4(-5.0, -8.0, -5.0, -6.0)

vec4 test_vec4_add_zero() {
    vec4 a = vec4(42.0, 17.0, 23.0, 8.0);
    vec4 b = vec4(0.0, 0.0, 0.0, 0.0);
    return a + b;
}

// run: test_vec4_add_zero() ~= vec4(42.0, 17.0, 23.0, 8.0)

vec4 test_vec4_add_variables() {
    vec4 a = vec4(15.0, 10.0, 5.0, 12.0);
    vec4 b = vec4(27.0, 5.0, 12.0, 3.0);
    return a + b;
}

// run: test_vec4_add_variables() ~= vec4(42.0, 15.0, 17.0, 15.0)

vec4 test_vec4_add_expressions() {
    return vec4(8.0, 4.0, 6.0, 2.0) + vec4(6.0, 2.0, 3.0, 4.0);
}

// run: test_vec4_add_expressions() ~= vec4(14.0, 6.0, 9.0, 6.0)

vec4 test_vec4_add_in_assignment() {
    vec4 result = vec4(5.0, 3.0, 2.0, 1.0);
    result = result + vec4(10.0, 7.0, 8.0, 9.0);
    return result;
}

// run: test_vec4_add_in_assignment() ~= vec4(15.0, 10.0, 10.0, 10.0)

vec4 test_vec4_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)
    // Addition saturates to max for each component
    vec4 a = vec4(100000.0, 50000.0, 25000.0, 10000.0);
    vec4 b = vec4(200000.0, 30000.0, 15000.0, 5000.0);
    return a + b;
}

// run: test_vec4_add_large_numbers() ~= vec4(32768.0, 32768.0, 32768.0, 15000.0)

vec4 test_vec4_add_mixed_components() {
    vec4 a = vec4(1.0, -2.0, 3.0, -4.0);
    vec4 b = vec4(-3.0, 4.0, -1.0, 2.0);
    return a + b;
}

// run: test_vec4_add_mixed_components() ~= vec4(-2.0, 2.0, 2.0, -2.0)

vec4 test_vec4_add_fractions() {
    vec4 a = vec4(1.5, 2.25, 3.75, 0.5);
    vec4 b = vec4(0.5, 1.75, 0.25, 1.5);
    return a + b;
}

// run: test_vec4_add_fractions() ~= vec4(2.0, 4.0, 4.0, 2.0)
