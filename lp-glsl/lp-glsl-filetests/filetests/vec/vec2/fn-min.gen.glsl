// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec2/fn-min --write
//
// test run
// target riscv32.q32

// ============================================================================
// Min: min(vec2, vec2) -> vec2 (component-wise minimum)
// ============================================================================

vec2 test_vec2_min_first_smaller() {
    // Function min() returns vec2 (component-wise minimum)
    vec2 a = vec2(3.0, 8.0);
    vec2 b = vec2(7.0, 4.0);
    return min(a, b);
}

// run: test_vec2_min_first_smaller() == vec2(3.0, 4.0)

vec2 test_vec2_min_second_smaller() {
    vec2 a = vec2(7.0, 8.0);
    vec2 b = vec2(3.0, 4.0);
    return min(a, b);
}

// run: test_vec2_min_second_smaller() == vec2(3.0, 4.0)

vec2 test_vec2_min_mixed() {
    vec2 a = vec2(3.0, 8.0);
    vec2 b = vec2(7.0, 4.0);
    return min(a, b);
}

// run: test_vec2_min_mixed() == vec2(3.0, 4.0)

vec2 test_vec2_min_equal() {
    vec2 a = vec2(5.0, 5.0);
    vec2 b = vec2(5.0, 5.0);
    return min(a, b);
}

// run: test_vec2_min_equal() == vec2(5.0, 5.0)

vec2 test_vec2_min_negative() {
    vec2 a = vec2(-3.0, -8.0);
    vec2 b = vec2(-7.0, -4.0);
    return min(a, b);
}

// run: test_vec2_min_negative() == vec2(-7.0, -8.0)

vec2 test_vec2_min_zero() {
    vec2 a = vec2(0.0, 5.0);
    vec2 b = vec2(2.0, -1.0);
    return min(a, b);
}

// run: test_vec2_min_zero() == vec2(0.0, -1.0)

vec2 test_vec2_min_variables() {
    vec2 a = vec2(10.0, 15.0);
    vec2 b = vec2(12.0, 10.0);
    return min(a, b);
}

// run: test_vec2_min_variables() == vec2(10.0, 10.0)

vec2 test_vec2_min_expressions() {
    return min(vec2(6.0, 2.0), vec2(4.0, 7.0));
}

// run: test_vec2_min_expressions() == vec2(4.0, 2.0)

vec2 test_vec2_min_in_expression() {
    vec2 a = vec2(3.0, 8.0);
    vec2 b = vec2(7.0, 4.0);
    vec2 c = vec2(1.0, 6.0);
    return min(a, min(b, c));
}

// run: test_vec2_min_in_expression() == vec2(1.0, 4.0)
