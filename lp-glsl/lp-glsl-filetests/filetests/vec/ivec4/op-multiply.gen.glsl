// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/op-multiply --write
//
// test run
// target riscv32.q32

// ============================================================================
// Multiply: ivec4 * ivec4 -> ivec4 (component-wise)
// ============================================================================

ivec4 test_ivec4_multiply_positive_positive() {
    // Multiplication with positive vectors (component-wise)
    ivec4 a = ivec4(6, 7, 2, 3);
    ivec4 b = ivec4(2, 3, 4, 5);
    return a * b;
}

// run: test_ivec4_multiply_positive_positive() == ivec4(12, 21, 8, 15)

ivec4 test_ivec4_multiply_positive_negative() {
    ivec4 a = ivec4(5, 4, 3, 2);
    ivec4 b = ivec4(-3, -2, -1, -4);
    return a * b;
}

// run: test_ivec4_multiply_positive_negative() == ivec4(-15, -8, -3, -8)

ivec4 test_ivec4_multiply_negative_negative() {
    ivec4 a = ivec4(-4, -5, -2, -3);
    ivec4 b = ivec4(-2, -3, -1, -2);
    return a * b;
}

// run: test_ivec4_multiply_negative_negative() == ivec4(8, 15, 2, 6)

ivec4 test_ivec4_multiply_by_zero() {
    ivec4 a = ivec4(123, 456, 789, 321);
    ivec4 b = ivec4(0, 0, 0, 0);
    return a * b;
}

// run: test_ivec4_multiply_by_zero() == ivec4(0, 0, 0, 0)

ivec4 test_ivec4_multiply_by_one() {
    ivec4 a = ivec4(42, 17, 23, 8);
    ivec4 b = ivec4(1, 1, 1, 1);
    return a * b;
}

// run: test_ivec4_multiply_by_one() == ivec4(42, 17, 23, 8)

ivec4 test_ivec4_multiply_variables() {
    ivec4 a = ivec4(8, 9, 7, 6);
    ivec4 b = ivec4(7, 6, 5, 4);
    return a * b;
}

// run: test_ivec4_multiply_variables() == ivec4(56, 54, 35, 24)

ivec4 test_ivec4_multiply_expressions() {
    return ivec4(3, 4, 5, 2) * ivec4(5, 2, 1, 6);
}

// run: test_ivec4_multiply_expressions() == ivec4(15, 8, 5, 12)

ivec4 test_ivec4_multiply_in_assignment() {
    ivec4 result = ivec4(6, 7, 8, 9);
    result = result * ivec4(2, 3, 1, 2);
    return result;
}

// run: test_ivec4_multiply_in_assignment() == ivec4(12, 21, 8, 18)

ivec4 test_ivec4_multiply_large_numbers() {
    ivec4 a = ivec4(1000, 2000, 3000, 4000);
    ivec4 b = ivec4(3000, 1000, 2000, 500);
    return a * b;
}

// run: test_ivec4_multiply_large_numbers() == ivec4(3000000, 2000000, 6000000, 2000000)

ivec4 test_ivec4_multiply_mixed_components() {
    ivec4 a = ivec4(2, -3, 4, -2);
    ivec4 b = ivec4(-4, 5, -2, 3);
    return a * b;
}

// run: test_ivec4_multiply_mixed_components() == ivec4(-8, -15, -8, -6)

ivec4 test_ivec4_multiply_fractions() {
    ivec4 a = ivec4(3, 4, 5, 2);
    ivec4 b = ivec4(5, 2, 1, 6);
    return a * b;
}

// run: test_ivec4_multiply_fractions() == ivec4(15, 8, 5, 12)
