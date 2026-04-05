// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lps-filetests-gen-app vec/vec3/op-subtract --write
//
// test run

// ============================================================================
// Subtract: vec3 - vec3 -> vec3 (component-wise)
// ============================================================================

vec3 test_vec3_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    vec3 a = vec3(5.0, 3.0, 2.0);
    vec3 b = vec3(2.0, 4.0, 1.0);
    return a - b;
}

// run: test_vec3_subtract_positive_positive() ~= vec3(3.0, -1.0, 1.0)

vec3 test_vec3_subtract_positive_negative() {
    vec3 a = vec3(10.0, 8.0, 5.0);
    vec3 b = vec3(-4.0, -2.0, -1.0);
    return a - b;
}

// run: test_vec3_subtract_positive_negative() ~= vec3(14.0, 10.0, 6.0)

vec3 test_vec3_subtract_negative_negative() {
    vec3 a = vec3(-3.0, -7.0, -2.0);
    vec3 b = vec3(-2.0, -1.0, -3.0);
    return a - b;
}

// run: test_vec3_subtract_negative_negative() ~= vec3(-1.0, -6.0, 1.0)

vec3 test_vec3_subtract_zero() {
    vec3 a = vec3(42.0, 17.0, 23.0);
    vec3 b = vec3(0.0, 0.0, 0.0);
    return a - b;
}

// run: test_vec3_subtract_zero() ~= vec3(42.0, 17.0, 23.0)

vec3 test_vec3_subtract_variables() {
    vec3 a = vec3(15.0, 10.0, 5.0);
    vec3 b = vec3(27.0, 5.0, 12.0);
    return a - b;
}

// run: test_vec3_subtract_variables() ~= vec3(-12.0, 5.0, -7.0)

vec3 test_vec3_subtract_expressions() {
    return vec3(8.0, 4.0, 6.0) - vec3(6.0, 2.0, 3.0);
}

// run: test_vec3_subtract_expressions() ~= vec3(2.0, 2.0, 3.0)

vec3 test_vec3_subtract_in_assignment() {
    vec3 result = vec3(5.0, 3.0, 2.0);
    result = result - vec3(10.0, 7.0, 8.0);
    return result;
}

// run: test_vec3_subtract_in_assignment() ~= vec3(-5.0, -4.0, -6.0)

vec3 test_vec3_subtract_large_numbers() {
    vec3 a = vec3(5000.0, 4000.0, 3000.0);
    vec3 b = vec3(1000.0, 500.0, 200.0);
    return a - b;
}

// run: test_vec3_subtract_large_numbers() ~= vec3(4000.0, 3500.0, 2800.0)

vec3 test_vec3_subtract_mixed_components() {
    vec3 a = vec3(1.0, -2.0, 3.0);
    vec3 b = vec3(-3.0, 4.0, -1.0);
    return a - b;
}

// run: test_vec3_subtract_mixed_components() ~= vec3(4.0, -6.0, 4.0)

vec3 test_vec3_subtract_fractions() {
    vec3 a = vec3(1.5, 2.25, 3.75);
    vec3 b = vec3(0.5, 1.75, 0.25);
    return a - b;
}

// run: test_vec3_subtract_fractions() ~= vec3(1.0, 0.5, 3.5)
