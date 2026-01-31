// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec2/fn-max --write
//
// test run
// target riscv32.q32

// ============================================================================
// Max: max(vec2, vec2) -> vec2 (component-wise maximum)
// ============================================================================

vec2 test_vec2_max_first_larger() {
    // Function max() returns vec2 (component-wise maximum)
    vec2 a = vec2(7.0, 8.0);
    vec2 b = vec2(3.0, 4.0);
    return max(a, b);
}

// run: test_vec2_max_first_larger() == vec2(7.0, 8.0)

vec2 test_vec2_max_second_larger() {
    vec2 a = vec2(3.0, 4.0);
    vec2 b = vec2(7.0, 8.0);
    return max(a, b);
}

// run: test_vec2_max_second_larger() == vec2(7.0, 8.0)

vec2 test_vec2_max_mixed() {
    vec2 a = vec2(3.0, 8.0);
    vec2 b = vec2(7.0, 4.0);
    return max(a, b);
}

// run: test_vec2_max_mixed() == vec2(7.0, 8.0)

vec2 test_vec2_max_equal() {
    vec2 a = vec2(5.0, 5.0);
    vec2 b = vec2(5.0, 5.0);
    return max(a, b);
}

// run: test_vec2_max_equal() == vec2(5.0, 5.0)

vec2 test_vec2_max_negative() {
    vec2 a = vec2(-3.0, -8.0);
    vec2 b = vec2(-7.0, -4.0);
    return max(a, b);
}

// run: test_vec2_max_negative() == vec2(-3.0, -4.0)

vec2 test_vec2_max_zero() {
    vec2 a = vec2(0.0, 5.0);
    vec2 b = vec2(2.0, -1.0);
    return max(a, b);
}

// run: test_vec2_max_zero() == vec2(2.0, 5.0)

vec2 test_vec2_max_variables() {
    vec2 a = vec2(10.0, 15.0);
    vec2 b = vec2(12.0, 10.0);
    return max(a, b);
}

// run: test_vec2_max_variables() == vec2(12.0, 15.0)

vec2 test_vec2_max_expressions() {
    return max(vec2(6.0, 2.0), vec2(4.0, 7.0));
}

// run: test_vec2_max_expressions() == vec2(6.0, 7.0)

vec2 test_vec2_max_in_expression() {
    vec2 a = vec2(3.0, 8.0);
    vec2 b = vec2(7.0, 4.0);
    vec2 c = vec2(1.0, 6.0);
    return max(a, max(b, c));
}

// run: test_vec2_max_in_expression() == vec2(7.0, 8.0)
