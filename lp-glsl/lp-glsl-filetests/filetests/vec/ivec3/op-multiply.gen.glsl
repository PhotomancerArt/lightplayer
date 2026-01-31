// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec3/op-multiply --write
//
// test run
// target riscv32.q32

// ============================================================================
// Multiply: ivec3 * ivec3 -> ivec3 (component-wise)
// ============================================================================

ivec3 test_ivec3_multiply_positive_positive() {
    // Multiplication with positive vectors (component-wise)
    ivec3 a = ivec3(6, 7, 2);
    ivec3 b = ivec3(2, 3, 4);
    return a * b;
}

// run: test_ivec3_multiply_positive_positive() == ivec3(12, 21, 8)

ivec3 test_ivec3_multiply_positive_negative() {
    ivec3 a = ivec3(5, 4, 3);
    ivec3 b = ivec3(-3, -2, -1);
    return a * b;
}

// run: test_ivec3_multiply_positive_negative() == ivec3(-15, -8, -3)

ivec3 test_ivec3_multiply_negative_negative() {
    ivec3 a = ivec3(-4, -5, -2);
    ivec3 b = ivec3(-2, -3, -1);
    return a * b;
}

// run: test_ivec3_multiply_negative_negative() == ivec3(8, 15, 2)

ivec3 test_ivec3_multiply_by_zero() {
    ivec3 a = ivec3(123, 456, 789);
    ivec3 b = ivec3(0, 0, 0);
    return a * b;
}

// run: test_ivec3_multiply_by_zero() == ivec3(0, 0, 0)

ivec3 test_ivec3_multiply_by_one() {
    ivec3 a = ivec3(42, 17, 23);
    ivec3 b = ivec3(1, 1, 1);
    return a * b;
}

// run: test_ivec3_multiply_by_one() == ivec3(42, 17, 23)

ivec3 test_ivec3_multiply_variables() {
    ivec3 a = ivec3(8, 9, 7);
    ivec3 b = ivec3(7, 6, 5);
    return a * b;
}

// run: test_ivec3_multiply_variables() == ivec3(56, 54, 35)

ivec3 test_ivec3_multiply_expressions() {
    return ivec3(3, 4, 5) * ivec3(5, 2, 1);
}

// run: test_ivec3_multiply_expressions() == ivec3(15, 8, 5)

ivec3 test_ivec3_multiply_in_assignment() {
    ivec3 result = ivec3(6, 7, 8);
    result = result * ivec3(2, 3, 1);
    return result;
}

// run: test_ivec3_multiply_in_assignment() == ivec3(12, 21, 8)

ivec3 test_ivec3_multiply_large_numbers() {
    ivec3 a = ivec3(1000, 2000, 3000);
    ivec3 b = ivec3(3000, 1000, 2000);
    return a * b;
}

// run: test_ivec3_multiply_large_numbers() == ivec3(3000000, 2000000, 6000000)

ivec3 test_ivec3_multiply_mixed_components() {
    ivec3 a = ivec3(2, -3, 4);
    ivec3 b = ivec3(-4, 5, -2);
    return a * b;
}

// run: test_ivec3_multiply_mixed_components() == ivec3(-8, -15, -8)

ivec3 test_ivec3_multiply_fractions() {
    ivec3 a = ivec3(3, 4, 5);
    ivec3 b = ivec3(5, 2, 1);
    return a * b;
}

// run: test_ivec3_multiply_fractions() == ivec3(15, 8, 5)
