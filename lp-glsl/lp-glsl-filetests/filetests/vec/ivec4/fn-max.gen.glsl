// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/fn-max --write
//
// test run
// target riscv32.q32

// ============================================================================
// Max: max(ivec4, ivec4) -> ivec4 (component-wise maximum)
// ============================================================================

ivec4 test_ivec4_max_first_larger() {
    // Function max() returns ivec4 (component-wise maximum)
    ivec4 a = ivec4(7, 8, 9, 6);
    ivec4 b = ivec4(3, 4, 5, 1);
    return max(a, b);
}

// run: test_ivec4_max_first_larger() == ivec4(7, 8, 9, 6)

ivec4 test_ivec4_max_second_larger() {
    ivec4 a = ivec4(3, 4, 5, 1);
    ivec4 b = ivec4(7, 8, 9, 6);
    return max(a, b);
}

// run: test_ivec4_max_second_larger() == ivec4(7, 8, 9, 6)

ivec4 test_ivec4_max_mixed() {
    ivec4 a = ivec4(3, 8, 2, 7);
    ivec4 b = ivec4(7, 4, 9, 3);
    return max(a, b);
}

// run: test_ivec4_max_mixed() == ivec4(7, 8, 9, 7)

ivec4 test_ivec4_max_equal() {
    ivec4 a = ivec4(5, 5, 5, 5);
    ivec4 b = ivec4(5, 5, 5, 5);
    return max(a, b);
}

// run: test_ivec4_max_equal() == ivec4(5, 5, 5, 5)

ivec4 test_ivec4_max_negative() {
    ivec4 a = ivec4(-3, -8, -2, -1);
    ivec4 b = ivec4(-7, -4, -9, -6);
    return max(a, b);
}

// run: test_ivec4_max_negative() == ivec4(-3, -4, -2, -1)

ivec4 test_ivec4_max_zero() {
    ivec4 a = ivec4(0, 5, -3, 2);
    ivec4 b = ivec4(2, -1, 0, -4);
    return max(a, b);
}

// run: test_ivec4_max_zero() == ivec4(2, 5, 0, 2)

ivec4 test_ivec4_max_variables() {
    ivec4 a = ivec4(10, 15, 8, 12);
    ivec4 b = ivec4(12, 10, 12, 8);
    return max(a, b);
}

// run: test_ivec4_max_variables() == ivec4(12, 15, 12, 12)

ivec4 test_ivec4_max_expressions() {
    return max(ivec4(6, 2, 8, 4), ivec4(4, 7, 3, 9));
}

// run: test_ivec4_max_expressions() == ivec4(6, 7, 8, 9)

ivec4 test_ivec4_max_in_expression() {
    ivec4 a = ivec4(3, 8, 5, 2);
    ivec4 b = ivec4(7, 4, 9, 7);
    ivec4 c = ivec4(1, 6, 2, 3);
    return max(a, max(b, c));
}

// run: test_ivec4_max_in_expression() == ivec4(7, 8, 9, 7)
