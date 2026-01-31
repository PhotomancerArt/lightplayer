// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec4/op-multiply --write
//
// test run
// target riscv32.q32

// ============================================================================
// Multiply: vec4 * vec4 -> vec4 (component-wise)
// ============================================================================

vec4 test_vec4_multiply_positive_positive() {
    // Multiplication with positive vectors (component-wise)
    vec4 a = vec4(6.0, 7.0, 2.0, 3.0);
    vec4 b = vec4(2.0, 3.0, 4.0, 5.0);
    return a * b;
}

// run: test_vec4_multiply_positive_positive() ~= vec4(12.0, 21.0, 8.0, 15.0)

vec4 test_vec4_multiply_positive_negative() {
    vec4 a = vec4(5.0, 4.0, 3.0, 2.0);
    vec4 b = vec4(-3.0, -2.0, -1.0, -4.0);
    return a * b;
}

// run: test_vec4_multiply_positive_negative() ~= vec4(-15.0, -8.0, -3.0, -8.0)

vec4 test_vec4_multiply_negative_negative() {
    vec4 a = vec4(-4.0, -5.0, -2.0, -3.0);
    vec4 b = vec4(-2.0, -3.0, -1.0, -2.0);
    return a * b;
}

// run: test_vec4_multiply_negative_negative() ~= vec4(8.0, 15.0, 2.0, 6.0)

vec4 test_vec4_multiply_by_zero() {
    vec4 a = vec4(123.0, 456.0, 789.0, 321.0);
    vec4 b = vec4(0.0, 0.0, 0.0, 0.0);
    return a * b;
}

// run: test_vec4_multiply_by_zero() ~= vec4(0.0, 0.0, 0.0, 0.0)

vec4 test_vec4_multiply_by_one() {
    vec4 a = vec4(42.0, 17.0, 23.0, 8.0);
    vec4 b = vec4(1.0, 1.0, 1.0, 1.0);
    return a * b;
}

// run: test_vec4_multiply_by_one() ~= vec4(42.0, 17.0, 23.0, 8.0)

vec4 test_vec4_multiply_variables() {
    vec4 a = vec4(8.0, 9.0, 7.0, 6.0);
    vec4 b = vec4(7.0, 6.0, 5.0, 4.0);
    return a * b;
}

// run: test_vec4_multiply_variables() ~= vec4(56.0, 54.0, 35.0, 24.0)

vec4 test_vec4_multiply_expressions() {
    return vec4(3.0, 4.0, 5.0, 2.0) * vec4(5.0, 2.0, 1.0, 6.0);
}

// run: test_vec4_multiply_expressions() ~= vec4(15.0, 8.0, 5.0, 12.0)

vec4 test_vec4_multiply_in_assignment() {
    vec4 result = vec4(6.0, 7.0, 8.0, 9.0);
    result = result * vec4(2.0, 3.0, 1.0, 2.0);
    return result;
}

// run: test_vec4_multiply_in_assignment() ~= vec4(12.0, 21.0, 8.0, 18.0)

vec4 test_vec4_multiply_large_numbers() {
    vec4 a = vec4(1000.0, 2000.0, 3000.0, 4000.0);
    vec4 b = vec4(3000.0, 1000.0, 2000.0, 500.0);
    return a * b;
}

// run: test_vec4_multiply_large_numbers() ~= vec4(32768.0, 32768.0, 32768.0, 32768.0)

vec4 test_vec4_multiply_mixed_components() {
    vec4 a = vec4(2.0, -3.0, 4.0, -2.0);
    vec4 b = vec4(-4.0, 5.0, -2.0, 3.0);
    return a * b;
}

// run: test_vec4_multiply_mixed_components() ~= vec4(-8.0, -15.0, -8.0, -6.0)

vec4 test_vec4_multiply_fractions() {
    vec4 a = vec4(1.5, 2.5, 3.5, 0.5);
    vec4 b = vec4(2.0, 0.5, 1.5, 4.0);
    return a * b;
}

// run: test_vec4_multiply_fractions() ~= vec4(3.0, 1.25, 5.25, 2.0)
