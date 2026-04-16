// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lps-filetests-gen-app vec/vec2/op-subtract --write
//
// test run

// ============================================================================
// Subtract: vec2 - vec2 -> vec2 (component-wise)
// ============================================================================

vec2 test_vec2_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    vec2 a = vec2(5.0, 3.0);
    vec2 b = vec2(2.0, 4.0);
    return a - b;
}

// run: test_vec2_subtract_positive_positive() ~= vec2(3.0, -1.0)

vec2 test_vec2_subtract_positive_negative() {
    vec2 a = vec2(10.0, 8.0);
    vec2 b = vec2(-4.0, -2.0);
    return a - b;
}

// run: test_vec2_subtract_positive_negative() ~= vec2(14.0, 10.0)

vec2 test_vec2_subtract_negative_negative() {
    vec2 a = vec2(-3.0, -7.0);
    vec2 b = vec2(-2.0, -1.0);
    return a - b;
}

// run: test_vec2_subtract_negative_negative() ~= vec2(-1.0, -6.0)

vec2 test_vec2_subtract_zero() {
    vec2 a = vec2(42.0, 17.0);
    vec2 b = vec2(0.0, 0.0);
    return a - b;
}

// run: test_vec2_subtract_zero() ~= vec2(42.0, 17.0)

vec2 test_vec2_subtract_variables() {
    vec2 a = vec2(15.0, 10.0);
    vec2 b = vec2(27.0, 5.0);
    return a - b;
}

// run: test_vec2_subtract_variables() ~= vec2(-12.0, 5.0)

vec2 test_vec2_subtract_expressions() {
    return vec2(8.0, 4.0) - vec2(6.0, 2.0);
}

// run: test_vec2_subtract_expressions() ~= vec2(2.0, 2.0)

vec2 test_vec2_subtract_in_assignment() {
    vec2 result = vec2(5.0, 3.0);
    result = result - vec2(10.0, 7.0);
    return result;
}

// run: test_vec2_subtract_in_assignment() ~= vec2(-5.0, -4.0)

vec2 test_vec2_subtract_large_numbers() {
    vec2 a = vec2(5000.0, 4000.0);
    vec2 b = vec2(1000.0, 3500.0);
    return a - b;
}

// run: test_vec2_subtract_large_numbers() ~= vec2(4000.0, 500.0)

vec2 test_vec2_subtract_mixed_components() {
    vec2 a = vec2(1.0, -2.0);
    vec2 b = vec2(-3.0, 4.0);
    return a - b;
}

// run: test_vec2_subtract_mixed_components() ~= vec2(4.0, -6.0)

vec2 test_vec2_subtract_fractions() {
    vec2 a = vec2(1.5, 2.25);
    vec2 b = vec2(0.5, 1.75);
    return a - b;
}

// run: test_vec2_subtract_fractions() ~= vec2(1.0, 0.5)
