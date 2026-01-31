// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec3/fn-max --write
//
// test run
// target riscv32.q32

// ============================================================================
// Max: max(ivec3, ivec3) -> ivec3 (component-wise maximum)
// ============================================================================

ivec3 test_ivec3_max_first_larger() {
    // Function max() returns ivec3 (component-wise maximum)
    ivec3 a = ivec3(7, 8, 9);
    ivec3 b = ivec3(3, 4, 5);
    return max(a, b);
}

// run: test_ivec3_max_first_larger() == ivec3(7, 8, 9)

ivec3 test_ivec3_max_second_larger() {
    ivec3 a = ivec3(3, 4, 5);
    ivec3 b = ivec3(7, 8, 9);
    return max(a, b);
}

// run: test_ivec3_max_second_larger() == ivec3(7, 8, 9)

ivec3 test_ivec3_max_mixed() {
    ivec3 a = ivec3(3, 8, 2);
    ivec3 b = ivec3(7, 4, 9);
    return max(a, b);
}

// run: test_ivec3_max_mixed() == ivec3(7, 8, 9)

ivec3 test_ivec3_max_equal() {
    ivec3 a = ivec3(5, 5, 5);
    ivec3 b = ivec3(5, 5, 5);
    return max(a, b);
}

// run: test_ivec3_max_equal() == ivec3(5, 5, 5)

ivec3 test_ivec3_max_negative() {
    ivec3 a = ivec3(-3, -8, -2);
    ivec3 b = ivec3(-7, -4, -9);
    return max(a, b);
}

// run: test_ivec3_max_negative() == ivec3(-3, -4, -2)

ivec3 test_ivec3_max_zero() {
    ivec3 a = ivec3(0, 5, -3);
    ivec3 b = ivec3(2, -1, 0);
    return max(a, b);
}

// run: test_ivec3_max_zero() == ivec3(2, 5, 0)

ivec3 test_ivec3_max_variables() {
    ivec3 a = ivec3(10, 15, 8);
    ivec3 b = ivec3(12, 10, 12);
    return max(a, b);
}

// run: test_ivec3_max_variables() == ivec3(12, 15, 12)

ivec3 test_ivec3_max_expressions() {
    return max(ivec3(6, 2, 8), ivec3(4, 7, 3));
}

// run: test_ivec3_max_expressions() == ivec3(6, 7, 8)

ivec3 test_ivec3_max_in_expression() {
    ivec3 a = ivec3(3, 8, 5);
    ivec3 b = ivec3(7, 4, 9);
    ivec3 c = ivec3(1, 6, 2);
    return max(a, max(b, c));
}

// run: test_ivec3_max_in_expression() == ivec3(7, 8, 9)
