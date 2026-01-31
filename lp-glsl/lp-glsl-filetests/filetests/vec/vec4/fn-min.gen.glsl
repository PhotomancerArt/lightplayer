// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec4/fn-min --write
//
// test run
// target riscv32.q32

// ============================================================================
// Min: min(vec4, vec4) -> vec4 (component-wise minimum)
// ============================================================================

vec4 test_vec4_min_first_smaller() {
    // Function min() returns vec4 (component-wise minimum)
    vec4 a = vec4(3.0, 8.0, 5.0, 1.0);
    vec4 b = vec4(7.0, 4.0, 9.0, 6.0);
    return min(a, b);
}

// run: test_vec4_min_first_smaller() == vec4(3.0, 4.0, 5.0, 1.0)

vec4 test_vec4_min_second_smaller() {
    vec4 a = vec4(7.0, 8.0, 9.0, 6.0);
    vec4 b = vec4(3.0, 4.0, 5.0, 1.0);
    return min(a, b);
}

// run: test_vec4_min_second_smaller() == vec4(3.0, 4.0, 5.0, 1.0)

vec4 test_vec4_min_mixed() {
    vec4 a = vec4(3.0, 8.0, 2.0, 7.0);
    vec4 b = vec4(7.0, 4.0, 9.0, 3.0);
    return min(a, b);
}

// run: test_vec4_min_mixed() == vec4(3.0, 4.0, 2.0, 3.0)

vec4 test_vec4_min_equal() {
    vec4 a = vec4(5.0, 5.0, 5.0, 5.0);
    vec4 b = vec4(5.0, 5.0, 5.0, 5.0);
    return min(a, b);
}

// run: test_vec4_min_equal() == vec4(5.0, 5.0, 5.0, 5.0)

vec4 test_vec4_min_negative() {
    vec4 a = vec4(-3.0, -8.0, -2.0, -1.0);
    vec4 b = vec4(-7.0, -4.0, -9.0, -6.0);
    return min(a, b);
}

// run: test_vec4_min_negative() == vec4(-7.0, -8.0, -9.0, -6.0)

vec4 test_vec4_min_zero() {
    vec4 a = vec4(0.0, 5.0, -3.0, 2.0);
    vec4 b = vec4(2.0, -1.0, 0.0, -4.0);
    return min(a, b);
}

// run: test_vec4_min_zero() == vec4(0.0, -1.0, -3.0, -4.0)

vec4 test_vec4_min_variables() {
    vec4 a = vec4(10.0, 15.0, 8.0, 12.0);
    vec4 b = vec4(12.0, 10.0, 12.0, 8.0);
    return min(a, b);
}

// run: test_vec4_min_variables() == vec4(10.0, 10.0, 8.0, 8.0)

vec4 test_vec4_min_expressions() {
    return min(vec4(6.0, 2.0, 8.0, 4.0), vec4(4.0, 7.0, 3.0, 9.0));
}

// run: test_vec4_min_expressions() == vec4(4.0, 2.0, 3.0, 4.0)

vec4 test_vec4_min_in_expression() {
    vec4 a = vec4(3.0, 8.0, 5.0, 2.0);
    vec4 b = vec4(7.0, 4.0, 9.0, 7.0);
    vec4 c = vec4(1.0, 6.0, 2.0, 3.0);
    return min(a, min(b, c));
}

// run: test_vec4_min_in_expression() == vec4(1.0, 4.0, 2.0, 2.0)
