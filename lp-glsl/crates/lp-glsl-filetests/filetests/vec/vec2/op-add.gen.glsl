// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec2/op-add --write
//
// test run
// target riscv32.q32

// ============================================================================
// Add: vec2 + vec2 -> vec2 (component-wise)
// ============================================================================

vec2 test_vec2_add_positive_positive() {
    // Addition with positive vectors (component-wise)
    vec2 a = vec2(5.0, 3.0);
    vec2 b = vec2(2.0, 4.0);
    return a + b;
}

// run: test_vec2_add_positive_positive() ~= vec2(7.0, 7.0)

vec2 test_vec2_add_positive_negative() {
    vec2 a = vec2(10.0, 8.0);
    vec2 b = vec2(-4.0, -2.0);
    return a + b;
}

// run: test_vec2_add_positive_negative() ~= vec2(6.0, 6.0)

vec2 test_vec2_add_negative_negative() {
    vec2 a = vec2(-3.0, -7.0);
    vec2 b = vec2(-2.0, -1.0);
    return a + b;
}

// run: test_vec2_add_negative_negative() ~= vec2(-5.0, -8.0)

vec2 test_vec2_add_zero() {
    vec2 a = vec2(42.0, 17.0);
    vec2 b = vec2(0.0, 0.0);
    return a + b;
}

// run: test_vec2_add_zero() ~= vec2(42.0, 17.0)

vec2 test_vec2_add_variables() {
    vec2 a = vec2(15.0, 10.0);
    vec2 b = vec2(27.0, 5.0);
    return a + b;
}

// run: test_vec2_add_variables() ~= vec2(42.0, 15.0)

vec2 test_vec2_add_expressions() {
    return vec2(8.0, 4.0) + vec2(6.0, 2.0);
}

// run: test_vec2_add_expressions() ~= vec2(14.0, 6.0)

vec2 test_vec2_add_in_assignment() {
    vec2 result = vec2(5.0, 3.0);
    result = result + vec2(10.0, 7.0);
    return result;
}

// run: test_vec2_add_in_assignment() ~= vec2(15.0, 10.0)

vec2 test_vec2_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)
    // Addition saturates to max for each component
    vec2 a = vec2(100000.0, 50000.0);
    vec2 b = vec2(200000.0, 30000.0);
    return a + b;
}

// run: test_vec2_add_large_numbers() ~= vec2(32768.0, 32768.0)

vec2 test_vec2_add_mixed_components() {
    vec2 a = vec2(1.0, -2.0);
    vec2 b = vec2(-3.0, 4.0);
    return a + b;
}

// run: test_vec2_add_mixed_components() ~= vec2(-2.0, 2.0)

vec2 test_vec2_add_fractions() {
    vec2 a = vec2(1.5, 2.25);
    vec2 b = vec2(0.5, 1.75);
    return a + b;
}

// run: test_vec2_add_fractions() ~= vec2(2.0, 4.0)
