// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec3/fn-min --write
//
// test run
// target riscv32.q32

// ============================================================================
// Min: min(ivec3, ivec3) -> ivec3 (component-wise minimum)
// ============================================================================

ivec3 test_ivec3_min_first_smaller() {
    // Function min() returns ivec3 (component-wise minimum)
    ivec3 a = ivec3(3, 8, 5);
    ivec3 b = ivec3(7, 4, 9);
    return min(a, b);
}

// run: test_ivec3_min_first_smaller() == ivec3(3, 4, 5)

ivec3 test_ivec3_min_second_smaller() {
    ivec3 a = ivec3(7, 8, 9);
    ivec3 b = ivec3(3, 4, 5);
    return min(a, b);
}

// run: test_ivec3_min_second_smaller() == ivec3(3, 4, 5)

ivec3 test_ivec3_min_mixed() {
    ivec3 a = ivec3(3, 8, 2);
    ivec3 b = ivec3(7, 4, 9);
    return min(a, b);
}

// run: test_ivec3_min_mixed() == ivec3(3, 4, 2)

ivec3 test_ivec3_min_equal() {
    ivec3 a = ivec3(5, 5, 5);
    ivec3 b = ivec3(5, 5, 5);
    return min(a, b);
}

// run: test_ivec3_min_equal() == ivec3(5, 5, 5)

ivec3 test_ivec3_min_negative() {
    ivec3 a = ivec3(-3, -8, -2);
    ivec3 b = ivec3(-7, -4, -9);
    return min(a, b);
}

// run: test_ivec3_min_negative() == ivec3(-7, -8, -9)

ivec3 test_ivec3_min_zero() {
    ivec3 a = ivec3(0, 5, -3);
    ivec3 b = ivec3(2, -1, 0);
    return min(a, b);
}

// run: test_ivec3_min_zero() == ivec3(0, -1, -3)

ivec3 test_ivec3_min_variables() {
    ivec3 a = ivec3(10, 15, 8);
    ivec3 b = ivec3(12, 10, 12);
    return min(a, b);
}

// run: test_ivec3_min_variables() == ivec3(10, 10, 8)

ivec3 test_ivec3_min_expressions() {
    return min(ivec3(6, 2, 8), ivec3(4, 7, 3));
}

// run: test_ivec3_min_expressions() == ivec3(4, 2, 3)

ivec3 test_ivec3_min_in_expression() {
    ivec3 a = ivec3(3, 8, 5);
    ivec3 b = ivec3(7, 4, 9);
    ivec3 c = ivec3(1, 6, 2);
    return min(a, min(b, c));
}

// run: test_ivec3_min_in_expression() == ivec3(1, 4, 2)
