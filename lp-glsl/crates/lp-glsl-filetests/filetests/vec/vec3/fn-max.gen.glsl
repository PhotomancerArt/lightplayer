// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec3/fn-max --write
//
// test run
// target riscv32.q32

// ============================================================================
// Max: max(vec3, vec3) -> vec3 (component-wise maximum)
// ============================================================================

vec3 test_vec3_max_first_larger() {
    // Function max() returns vec3 (component-wise maximum)
    vec3 a = vec3(7.0, 8.0, 9.0);
    vec3 b = vec3(3.0, 4.0, 5.0);
    return max(a, b);
}

// run: test_vec3_max_first_larger() == vec3(7.0, 8.0, 9.0)

vec3 test_vec3_max_second_larger() {
    vec3 a = vec3(3.0, 4.0, 5.0);
    vec3 b = vec3(7.0, 8.0, 9.0);
    return max(a, b);
}

// run: test_vec3_max_second_larger() == vec3(7.0, 8.0, 9.0)

vec3 test_vec3_max_mixed() {
    vec3 a = vec3(3.0, 8.0, 2.0);
    vec3 b = vec3(7.0, 4.0, 9.0);
    return max(a, b);
}

// run: test_vec3_max_mixed() == vec3(7.0, 8.0, 9.0)

vec3 test_vec3_max_equal() {
    vec3 a = vec3(5.0, 5.0, 5.0);
    vec3 b = vec3(5.0, 5.0, 5.0);
    return max(a, b);
}

// run: test_vec3_max_equal() == vec3(5.0, 5.0, 5.0)

vec3 test_vec3_max_negative() {
    vec3 a = vec3(-3.0, -8.0, -2.0);
    vec3 b = vec3(-7.0, -4.0, -9.0);
    return max(a, b);
}

// run: test_vec3_max_negative() == vec3(-3.0, -4.0, -2.0)

vec3 test_vec3_max_zero() {
    vec3 a = vec3(0.0, 5.0, -3.0);
    vec3 b = vec3(2.0, -1.0, 0.0);
    return max(a, b);
}

// run: test_vec3_max_zero() == vec3(2.0, 5.0, 0.0)

vec3 test_vec3_max_variables() {
    vec3 a = vec3(10.0, 15.0, 8.0);
    vec3 b = vec3(12.0, 10.0, 12.0);
    return max(a, b);
}

// run: test_vec3_max_variables() == vec3(12.0, 15.0, 12.0)

vec3 test_vec3_max_expressions() {
    return max(vec3(6.0, 2.0, 8.0), vec3(4.0, 7.0, 3.0));
}

// run: test_vec3_max_expressions() == vec3(6.0, 7.0, 8.0)

vec3 test_vec3_max_in_expression() {
    vec3 a = vec3(3.0, 8.0, 5.0);
    vec3 b = vec3(7.0, 4.0, 9.0);
    vec3 c = vec3(1.0, 6.0, 2.0);
    return max(a, max(b, c));
}

// run: test_vec3_max_in_expression() == vec3(7.0, 8.0, 9.0)
