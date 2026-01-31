// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec2/fn-max --write
//
// test run
// target riscv32.q32

// ============================================================================
// Max: max(ivec2, ivec2) -> ivec2 (component-wise maximum)
// ============================================================================

ivec2 test_ivec2_max_first_larger() {
    // Function max() returns ivec2 (component-wise maximum)
    ivec2 a = ivec2(7, 8);
    ivec2 b = ivec2(3, 4);
    return max(a, b);
}

// run: test_ivec2_max_first_larger() == ivec2(7, 8)

ivec2 test_ivec2_max_second_larger() {
    ivec2 a = ivec2(3, 4);
    ivec2 b = ivec2(7, 8);
    return max(a, b);
}

// run: test_ivec2_max_second_larger() == ivec2(7, 8)

ivec2 test_ivec2_max_mixed() {
    ivec2 a = ivec2(3, 8);
    ivec2 b = ivec2(7, 4);
    return max(a, b);
}

// run: test_ivec2_max_mixed() == ivec2(7, 8)

ivec2 test_ivec2_max_equal() {
    ivec2 a = ivec2(5, 5);
    ivec2 b = ivec2(5, 5);
    return max(a, b);
}

// run: test_ivec2_max_equal() == ivec2(5, 5)

ivec2 test_ivec2_max_negative() {
    ivec2 a = ivec2(-3, -8);
    ivec2 b = ivec2(-7, -4);
    return max(a, b);
}

// run: test_ivec2_max_negative() == ivec2(-3, -4)

ivec2 test_ivec2_max_zero() {
    ivec2 a = ivec2(0, 5);
    ivec2 b = ivec2(2, -1);
    return max(a, b);
}

// run: test_ivec2_max_zero() == ivec2(2, 5)

ivec2 test_ivec2_max_variables() {
    ivec2 a = ivec2(10, 15);
    ivec2 b = ivec2(12, 10);
    return max(a, b);
}

// run: test_ivec2_max_variables() == ivec2(12, 15)

ivec2 test_ivec2_max_expressions() {
    return max(ivec2(6, 2), ivec2(4, 7));
}

// run: test_ivec2_max_expressions() == ivec2(6, 7)

ivec2 test_ivec2_max_in_expression() {
    ivec2 a = ivec2(3, 8);
    ivec2 b = ivec2(7, 4);
    ivec2 c = ivec2(1, 6);
    return max(a, max(b, c));
}

// run: test_ivec2_max_in_expression() == ivec2(7, 8)
