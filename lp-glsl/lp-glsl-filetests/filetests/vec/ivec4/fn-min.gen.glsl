// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/fn-min --write
//
// test run
// target riscv32.q32

// ============================================================================
// Min: min(ivec4, ivec4) -> ivec4 (component-wise minimum)
// ============================================================================

ivec4 test_ivec4_min_first_smaller() {
    // Function min() returns ivec4 (component-wise minimum)
    ivec4 a = ivec4(3, 8, 5, 1);
    ivec4 b = ivec4(7, 4, 9, 6);
    return min(a, b);
}

// run: test_ivec4_min_first_smaller() == ivec4(3, 4, 5, 1)

ivec4 test_ivec4_min_second_smaller() {
    ivec4 a = ivec4(7, 8, 9, 6);
    ivec4 b = ivec4(3, 4, 5, 1);
    return min(a, b);
}

// run: test_ivec4_min_second_smaller() == ivec4(3, 4, 5, 1)

ivec4 test_ivec4_min_mixed() {
    ivec4 a = ivec4(3, 8, 2, 7);
    ivec4 b = ivec4(7, 4, 9, 3);
    return min(a, b);
}

// run: test_ivec4_min_mixed() == ivec4(3, 4, 2, 3)

ivec4 test_ivec4_min_equal() {
    ivec4 a = ivec4(5, 5, 5, 5);
    ivec4 b = ivec4(5, 5, 5, 5);
    return min(a, b);
}

// run: test_ivec4_min_equal() == ivec4(5, 5, 5, 5)

ivec4 test_ivec4_min_negative() {
    ivec4 a = ivec4(-3, -8, -2, -1);
    ivec4 b = ivec4(-7, -4, -9, -6);
    return min(a, b);
}

// run: test_ivec4_min_negative() == ivec4(-7, -8, -9, -6)

ivec4 test_ivec4_min_zero() {
    ivec4 a = ivec4(0, 5, -3, 2);
    ivec4 b = ivec4(2, -1, 0, -4);
    return min(a, b);
}

// run: test_ivec4_min_zero() == ivec4(0, -1, -3, -4)

ivec4 test_ivec4_min_variables() {
    ivec4 a = ivec4(10, 15, 8, 12);
    ivec4 b = ivec4(12, 10, 12, 8);
    return min(a, b);
}

// run: test_ivec4_min_variables() == ivec4(10, 10, 8, 8)

ivec4 test_ivec4_min_expressions() {
    return min(ivec4(6, 2, 8, 4), ivec4(4, 7, 3, 9));
}

// run: test_ivec4_min_expressions() == ivec4(4, 2, 3, 4)

ivec4 test_ivec4_min_in_expression() {
    ivec4 a = ivec4(3, 8, 5, 2);
    ivec4 b = ivec4(7, 4, 9, 7);
    ivec4 c = ivec4(1, 6, 2, 3);
    return min(a, min(b, c));
}

// run: test_ivec4_min_in_expression() == ivec4(1, 4, 2, 2)
