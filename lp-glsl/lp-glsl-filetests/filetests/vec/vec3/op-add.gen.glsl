// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec3/op-add --write
//
// test run
// target riscv32.q32

// ============================================================================
// Add: vec3 + vec3 -> vec3 (component-wise)
// ============================================================================

vec3 test_vec3_add_positive_positive() {
    // Addition with positive vectors (component-wise)
    vec3 a = vec3(5.0, 3.0, 2.0);
    vec3 b = vec3(2.0, 4.0, 1.0);
    return a + b;
}

// run: test_vec3_add_positive_positive() ~= vec3(7.0, 7.0, 3.0)

vec3 test_vec3_add_positive_negative() {
    vec3 a = vec3(10.0, 8.0, 5.0);
    vec3 b = vec3(-4.0, -2.0, -1.0);
    return a + b;
}

// run: test_vec3_add_positive_negative() ~= vec3(6.0, 6.0, 4.0)

vec3 test_vec3_add_negative_negative() {
    vec3 a = vec3(-3.0, -7.0, -2.0);
    vec3 b = vec3(-2.0, -1.0, -3.0);
    return a + b;
}

// run: test_vec3_add_negative_negative() ~= vec3(-5.0, -8.0, -5.0)

vec3 test_vec3_add_zero() {
    vec3 a = vec3(42.0, 17.0, 23.0);
    vec3 b = vec3(0.0, 0.0, 0.0);
    return a + b;
}

// run: test_vec3_add_zero() ~= vec3(42.0, 17.0, 23.0)

vec3 test_vec3_add_variables() {
    vec3 a = vec3(15.0, 10.0, 5.0);
    vec3 b = vec3(27.0, 5.0, 12.0);
    return a + b;
}

// run: test_vec3_add_variables() ~= vec3(42.0, 15.0, 17.0)

vec3 test_vec3_add_expressions() {
    return vec3(8.0, 4.0, 6.0) + vec3(6.0, 2.0, 3.0);
}

// run: test_vec3_add_expressions() ~= vec3(14.0, 6.0, 9.0)

vec3 test_vec3_add_in_assignment() {
    vec3 result = vec3(5.0, 3.0, 2.0);
    result = result + vec3(10.0, 7.0, 8.0);
    return result;
}

// run: test_vec3_add_in_assignment() ~= vec3(15.0, 10.0, 10.0)

vec3 test_vec3_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)
    // Addition saturates to max for each component
    vec3 a = vec3(100000.0, 50000.0, 25000.0);
    vec3 b = vec3(200000.0, 30000.0, 15000.0);
    return a + b;
}

// run: test_vec3_add_large_numbers() ~= vec3(32768.0, 32768.0, 32768.0)

vec3 test_vec3_add_mixed_components() {
    vec3 a = vec3(1.0, -2.0, 3.0);
    vec3 b = vec3(-3.0, 4.0, -1.0);
    return a + b;
}

// run: test_vec3_add_mixed_components() ~= vec3(-2.0, 2.0, 2.0)

vec3 test_vec3_add_fractions() {
    vec3 a = vec3(1.5, 2.25, 3.75);
    vec3 b = vec3(0.5, 1.75, 0.25);
    return a + b;
}

// run: test_vec3_add_fractions() ~= vec3(2.0, 4.0, 4.0)
