// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec3/fn-min --write
//
// test run
// target riscv32.q32

// ============================================================================
// Min: min(vec3, vec3) -> vec3 (component-wise minimum)
// ============================================================================

vec3 test_vec3_min_first_smaller() {
    // Function min() returns vec3 (component-wise minimum)
    vec3 a = vec3(3.0, 8.0, 5.0);
    vec3 b = vec3(7.0, 4.0, 9.0);
    return min(a, b);
}

// run: test_vec3_min_first_smaller() == vec3(3.0, 4.0, 5.0)

vec3 test_vec3_min_second_smaller() {
    vec3 a = vec3(7.0, 8.0, 9.0);
    vec3 b = vec3(3.0, 4.0, 5.0);
    return min(a, b);
}

// run: test_vec3_min_second_smaller() == vec3(3.0, 4.0, 5.0)

vec3 test_vec3_min_mixed() {
    vec3 a = vec3(3.0, 8.0, 2.0);
    vec3 b = vec3(7.0, 4.0, 9.0);
    return min(a, b);
}

// run: test_vec3_min_mixed() == vec3(3.0, 4.0, 2.0)

vec3 test_vec3_min_equal() {
    vec3 a = vec3(5.0, 5.0, 5.0);
    vec3 b = vec3(5.0, 5.0, 5.0);
    return min(a, b);
}

// run: test_vec3_min_equal() == vec3(5.0, 5.0, 5.0)

vec3 test_vec3_min_negative() {
    vec3 a = vec3(-3.0, -8.0, -2.0);
    vec3 b = vec3(-7.0, -4.0, -9.0);
    return min(a, b);
}

// run: test_vec3_min_negative() == vec3(-7.0, -8.0, -9.0)

vec3 test_vec3_min_zero() {
    vec3 a = vec3(0.0, 5.0, -3.0);
    vec3 b = vec3(2.0, -1.0, 0.0);
    return min(a, b);
}

// run: test_vec3_min_zero() == vec3(0.0, -1.0, -3.0)

vec3 test_vec3_min_variables() {
    vec3 a = vec3(10.0, 15.0, 8.0);
    vec3 b = vec3(12.0, 10.0, 12.0);
    return min(a, b);
}

// run: test_vec3_min_variables() == vec3(10.0, 10.0, 8.0)

vec3 test_vec3_min_expressions() {
    return min(vec3(6.0, 2.0, 8.0), vec3(4.0, 7.0, 3.0));
}

// run: test_vec3_min_expressions() == vec3(4.0, 2.0, 3.0)

vec3 test_vec3_min_in_expression() {
    vec3 a = vec3(3.0, 8.0, 5.0);
    vec3 b = vec3(7.0, 4.0, 9.0);
    vec3 c = vec3(1.0, 6.0, 2.0);
    return min(a, min(b, c));
}

// run: test_vec3_min_in_expression() == vec3(1.0, 4.0, 2.0)
