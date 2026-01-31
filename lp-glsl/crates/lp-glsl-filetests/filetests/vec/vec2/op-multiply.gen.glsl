// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec2/op-multiply --write
//
// test run
// target riscv32.q32

// ============================================================================
// Multiply: vec2 * vec2 -> vec2 (component-wise)
// ============================================================================

vec2 test_vec2_multiply_positive_positive() {
    // Multiplication with positive vectors (component-wise)
    vec2 a = vec2(6.0, 7.0);
    vec2 b = vec2(2.0, 3.0);
    return a * b;
}

// run: test_vec2_multiply_positive_positive() ~= vec2(12.0, 21.0)

vec2 test_vec2_multiply_positive_negative() {
    vec2 a = vec2(5.0, 4.0);
    vec2 b = vec2(-3.0, -2.0);
    return a * b;
}

// run: test_vec2_multiply_positive_negative() ~= vec2(-15.0, -8.0)

vec2 test_vec2_multiply_negative_negative() {
    vec2 a = vec2(-4.0, -5.0);
    vec2 b = vec2(-2.0, -3.0);
    return a * b;
}

// run: test_vec2_multiply_negative_negative() ~= vec2(8.0, 15.0)

vec2 test_vec2_multiply_by_zero() {
    vec2 a = vec2(123.0, 456.0);
    vec2 b = vec2(0.0, 0.0);
    return a * b;
}

// run: test_vec2_multiply_by_zero() ~= vec2(0.0, 0.0)

vec2 test_vec2_multiply_by_one() {
    vec2 a = vec2(42.0, 17.0);
    vec2 b = vec2(1.0, 1.0);
    return a * b;
}

// run: test_vec2_multiply_by_one() ~= vec2(42.0, 17.0)

vec2 test_vec2_multiply_variables() {
    vec2 a = vec2(8.0, 9.0);
    vec2 b = vec2(7.0, 6.0);
    return a * b;
}

// run: test_vec2_multiply_variables() ~= vec2(56.0, 54.0)

vec2 test_vec2_multiply_expressions() {
    return vec2(3.0, 4.0) * vec2(5.0, 2.0);
}

// run: test_vec2_multiply_expressions() ~= vec2(15.0, 8.0)

vec2 test_vec2_multiply_in_assignment() {
    vec2 result = vec2(6.0, 7.0);
    result = result * vec2(2.0, 3.0);
    return result;
}

// run: test_vec2_multiply_in_assignment() ~= vec2(12.0, 21.0)

vec2 test_vec2_multiply_large_numbers() {
    vec2 a = vec2(1000.0, 2000.0);
    vec2 b = vec2(3000.0, 1000.0);
    return a * b;
}

// run: test_vec2_multiply_large_numbers() ~= vec2(32768.0, 32768.0)

vec2 test_vec2_multiply_mixed_components() {
    vec2 a = vec2(2.0, -3.0);
    vec2 b = vec2(-4.0, 5.0);
    return a * b;
}

// run: test_vec2_multiply_mixed_components() ~= vec2(-8.0, -15.0)

vec2 test_vec2_multiply_fractions() {
    vec2 a = vec2(1.5, 2.5);
    vec2 b = vec2(2.0, 0.5);
    return a * b;
}

// run: test_vec2_multiply_fractions() ~= vec2(3.0, 1.25)
