// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec3/op-multiply --write
//
// test run
// target riscv32.q32

// ============================================================================
// Multiply: vec3 * vec3 -> vec3 (component-wise)
// ============================================================================

vec3 test_vec3_multiply_positive_positive() {
    // Multiplication with positive vectors (component-wise)
    vec3 a = vec3(6.0, 7.0, 2.0);
    vec3 b = vec3(2.0, 3.0, 4.0);
    return a * b;
}

// run: test_vec3_multiply_positive_positive() ~= vec3(12.0, 21.0, 8.0)

vec3 test_vec3_multiply_positive_negative() {
    vec3 a = vec3(5.0, 4.0, 3.0);
    vec3 b = vec3(-3.0, -2.0, -1.0);
    return a * b;
}

// run: test_vec3_multiply_positive_negative() ~= vec3(-15.0, -8.0, -3.0)

vec3 test_vec3_multiply_negative_negative() {
    vec3 a = vec3(-4.0, -5.0, -2.0);
    vec3 b = vec3(-2.0, -3.0, -1.0);
    return a * b;
}

// run: test_vec3_multiply_negative_negative() ~= vec3(8.0, 15.0, 2.0)

vec3 test_vec3_multiply_by_zero() {
    vec3 a = vec3(123.0, 456.0, 789.0);
    vec3 b = vec3(0.0, 0.0, 0.0);
    return a * b;
}

// run: test_vec3_multiply_by_zero() ~= vec3(0.0, 0.0, 0.0)

vec3 test_vec3_multiply_by_one() {
    vec3 a = vec3(42.0, 17.0, 23.0);
    vec3 b = vec3(1.0, 1.0, 1.0);
    return a * b;
}

// run: test_vec3_multiply_by_one() ~= vec3(42.0, 17.0, 23.0)

vec3 test_vec3_multiply_variables() {
    vec3 a = vec3(8.0, 9.0, 7.0);
    vec3 b = vec3(7.0, 6.0, 5.0);
    return a * b;
}

// run: test_vec3_multiply_variables() ~= vec3(56.0, 54.0, 35.0)

vec3 test_vec3_multiply_expressions() {
    return vec3(3.0, 4.0, 5.0) * vec3(5.0, 2.0, 1.0);
}

// run: test_vec3_multiply_expressions() ~= vec3(15.0, 8.0, 5.0)

vec3 test_vec3_multiply_in_assignment() {
    vec3 result = vec3(6.0, 7.0, 8.0);
    result = result * vec3(2.0, 3.0, 1.0);
    return result;
}

// run: test_vec3_multiply_in_assignment() ~= vec3(12.0, 21.0, 8.0)

vec3 test_vec3_multiply_large_numbers() {
    vec3 a = vec3(1000.0, 2000.0, 3000.0);
    vec3 b = vec3(3000.0, 1000.0, 2000.0);
    return a * b;
}

// run: test_vec3_multiply_large_numbers() ~= vec3(32768.0, 32768.0, 32768.0)

vec3 test_vec3_multiply_mixed_components() {
    vec3 a = vec3(2.0, -3.0, 4.0);
    vec3 b = vec3(-4.0, 5.0, -2.0);
    return a * b;
}

// run: test_vec3_multiply_mixed_components() ~= vec3(-8.0, -15.0, -8.0)

vec3 test_vec3_multiply_fractions() {
    vec3 a = vec3(1.5, 2.5, 3.5);
    vec3 b = vec3(2.0, 0.5, 1.5);
    return a * b;
}

// run: test_vec3_multiply_fractions() ~= vec3(3.0, 1.25, 5.25)
