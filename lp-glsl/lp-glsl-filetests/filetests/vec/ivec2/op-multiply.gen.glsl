// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec2/op-multiply --write
//
// test run
// target riscv32.q32

// ============================================================================
// Multiply: ivec2 * ivec2 -> ivec2 (component-wise)
// ============================================================================

ivec2 test_ivec2_multiply_positive_positive() {
    // Multiplication with positive vectors (component-wise)
    ivec2 a = ivec2(6, 7);
    ivec2 b = ivec2(2, 3);
    return a * b;
}

// run: test_ivec2_multiply_positive_positive() == ivec2(12, 21)

ivec2 test_ivec2_multiply_positive_negative() {
    ivec2 a = ivec2(5, 4);
    ivec2 b = ivec2(-3, -2);
    return a * b;
}

// run: test_ivec2_multiply_positive_negative() == ivec2(-15, -8)

ivec2 test_ivec2_multiply_negative_negative() {
    ivec2 a = ivec2(-4, -5);
    ivec2 b = ivec2(-2, -3);
    return a * b;
}

// run: test_ivec2_multiply_negative_negative() == ivec2(8, 15)

ivec2 test_ivec2_multiply_by_zero() {
    ivec2 a = ivec2(123, 456);
    ivec2 b = ivec2(0, 0);
    return a * b;
}

// run: test_ivec2_multiply_by_zero() == ivec2(0, 0)

ivec2 test_ivec2_multiply_by_one() {
    ivec2 a = ivec2(42, 17);
    ivec2 b = ivec2(1, 1);
    return a * b;
}

// run: test_ivec2_multiply_by_one() == ivec2(42, 17)

ivec2 test_ivec2_multiply_variables() {
    ivec2 a = ivec2(8, 9);
    ivec2 b = ivec2(7, 6);
    return a * b;
}

// run: test_ivec2_multiply_variables() == ivec2(56, 54)

ivec2 test_ivec2_multiply_expressions() {
    return ivec2(3, 4) * ivec2(5, 2);
}

// run: test_ivec2_multiply_expressions() == ivec2(15, 8)

ivec2 test_ivec2_multiply_in_assignment() {
    ivec2 result = ivec2(6, 7);
    result = result * ivec2(2, 3);
    return result;
}

// run: test_ivec2_multiply_in_assignment() == ivec2(12, 21)

ivec2 test_ivec2_multiply_large_numbers() {
    ivec2 a = ivec2(1000, 2000);
    ivec2 b = ivec2(3000, 1000);
    return a * b;
}

// run: test_ivec2_multiply_large_numbers() == ivec2(3000000, 2000000)

ivec2 test_ivec2_multiply_mixed_components() {
    ivec2 a = ivec2(2, -3);
    ivec2 b = ivec2(-4, 5);
    return a * b;
}

// run: test_ivec2_multiply_mixed_components() == ivec2(-8, -15)

ivec2 test_ivec2_multiply_fractions() {
    ivec2 a = ivec2(3, 4);
    ivec2 b = ivec2(5, 2);
    return a * b;
}

// run: test_ivec2_multiply_fractions() == ivec2(15, 8)
