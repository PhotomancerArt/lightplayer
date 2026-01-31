// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec4/fn-max --write
//
// test run
// target riscv32.q32

// ============================================================================
// Max: max(vec4, vec4) -> vec4 (component-wise maximum)
// ============================================================================

vec4 test_vec4_max_first_larger() {
    // Function max() returns vec4 (component-wise maximum)
    vec4 a = vec4(7.0, 8.0, 9.0, 6.0);
    vec4 b = vec4(3.0, 4.0, 5.0, 1.0);
    return max(a, b);
}

// run: test_vec4_max_first_larger() == vec4(7.0, 8.0, 9.0, 6.0)

vec4 test_vec4_max_second_larger() {
    vec4 a = vec4(3.0, 4.0, 5.0, 1.0);
    vec4 b = vec4(7.0, 8.0, 9.0, 6.0);
    return max(a, b);
}

// run: test_vec4_max_second_larger() == vec4(7.0, 8.0, 9.0, 6.0)

vec4 test_vec4_max_mixed() {
    vec4 a = vec4(3.0, 8.0, 2.0, 7.0);
    vec4 b = vec4(7.0, 4.0, 9.0, 3.0);
    return max(a, b);
}

// run: test_vec4_max_mixed() == vec4(7.0, 8.0, 9.0, 7.0)

vec4 test_vec4_max_equal() {
    vec4 a = vec4(5.0, 5.0, 5.0, 5.0);
    vec4 b = vec4(5.0, 5.0, 5.0, 5.0);
    return max(a, b);
}

// run: test_vec4_max_equal() == vec4(5.0, 5.0, 5.0, 5.0)

vec4 test_vec4_max_negative() {
    vec4 a = vec4(-3.0, -8.0, -2.0, -1.0);
    vec4 b = vec4(-7.0, -4.0, -9.0, -6.0);
    return max(a, b);
}

// run: test_vec4_max_negative() == vec4(-3.0, -4.0, -2.0, -1.0)

vec4 test_vec4_max_zero() {
    vec4 a = vec4(0.0, 5.0, -3.0, 2.0);
    vec4 b = vec4(2.0, -1.0, 0.0, -4.0);
    return max(a, b);
}

// run: test_vec4_max_zero() == vec4(2.0, 5.0, 0.0, 2.0)

vec4 test_vec4_max_variables() {
    vec4 a = vec4(10.0, 15.0, 8.0, 12.0);
    vec4 b = vec4(12.0, 10.0, 12.0, 8.0);
    return max(a, b);
}

// run: test_vec4_max_variables() == vec4(12.0, 15.0, 12.0, 12.0)

vec4 test_vec4_max_expressions() {
    return max(vec4(6.0, 2.0, 8.0, 4.0), vec4(4.0, 7.0, 3.0, 9.0));
}

// run: test_vec4_max_expressions() == vec4(6.0, 7.0, 8.0, 9.0)

vec4 test_vec4_max_in_expression() {
    vec4 a = vec4(3.0, 8.0, 5.0, 2.0);
    vec4 b = vec4(7.0, 4.0, 9.0, 7.0);
    vec4 c = vec4(1.0, 6.0, 2.0, 3.0);
    return max(a, max(b, c));
}

// run: test_vec4_max_in_expression() == vec4(7.0, 8.0, 9.0, 7.0)
