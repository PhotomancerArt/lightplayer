// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec2/fn-min --write
//
// test run
// target riscv32.q32

// ============================================================================
// Min: min(ivec2, ivec2) -> ivec2 (component-wise minimum)
// ============================================================================

ivec2 test_ivec2_min_first_smaller() {
    // Function min() returns ivec2 (component-wise minimum)
    ivec2 a = ivec2(3, 8);
    ivec2 b = ivec2(7, 4);
    return min(a, b);
}

// run: test_ivec2_min_first_smaller() == ivec2(3, 4)

ivec2 test_ivec2_min_second_smaller() {
    ivec2 a = ivec2(7, 8);
    ivec2 b = ivec2(3, 4);
    return min(a, b);
}

// run: test_ivec2_min_second_smaller() == ivec2(3, 4)

ivec2 test_ivec2_min_mixed() {
    ivec2 a = ivec2(3, 8);
    ivec2 b = ivec2(7, 4);
    return min(a, b);
}

// run: test_ivec2_min_mixed() == ivec2(3, 4)

ivec2 test_ivec2_min_equal() {
    ivec2 a = ivec2(5, 5);
    ivec2 b = ivec2(5, 5);
    return min(a, b);
}

// run: test_ivec2_min_equal() == ivec2(5, 5)

ivec2 test_ivec2_min_negative() {
    ivec2 a = ivec2(-3, -8);
    ivec2 b = ivec2(-7, -4);
    return min(a, b);
}

// run: test_ivec2_min_negative() == ivec2(-7, -8)

ivec2 test_ivec2_min_zero() {
    ivec2 a = ivec2(0, 5);
    ivec2 b = ivec2(2, -1);
    return min(a, b);
}

// run: test_ivec2_min_zero() == ivec2(0, -1)

ivec2 test_ivec2_min_variables() {
    ivec2 a = ivec2(10, 15);
    ivec2 b = ivec2(12, 10);
    return min(a, b);
}

// run: test_ivec2_min_variables() == ivec2(10, 10)

ivec2 test_ivec2_min_expressions() {
    return min(ivec2(6, 2), ivec2(4, 7));
}

// run: test_ivec2_min_expressions() == ivec2(4, 2)

ivec2 test_ivec2_min_in_expression() {
    ivec2 a = ivec2(3, 8);
    ivec2 b = ivec2(7, 4);
    ivec2 c = ivec2(1, 6);
    return min(a, min(b, c));
}

// run: test_ivec2_min_in_expression() == ivec2(1, 4)
