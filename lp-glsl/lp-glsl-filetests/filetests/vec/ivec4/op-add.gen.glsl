// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/op-add --write
//
// test run
// target riscv32.q32

// ============================================================================
// Add: ivec4 + ivec4 -> ivec4 (component-wise)
// ============================================================================

ivec4 test_ivec4_add_positive_positive() {
    // Addition with positive vectors (component-wise)
    ivec4 a = ivec4(5, 3, 2, 1);
    ivec4 b = ivec4(2, 4, 1, 3);
    return a + b;
}

// run: test_ivec4_add_positive_positive() == ivec4(7, 7, 3, 4)

ivec4 test_ivec4_add_positive_negative() {
    ivec4 a = ivec4(10, 8, 5, 3);
    ivec4 b = ivec4(-4, -2, -1, -3);
    return a + b;
}

// run: test_ivec4_add_positive_negative() == ivec4(6, 6, 4, 0)

ivec4 test_ivec4_add_negative_negative() {
    ivec4 a = ivec4(-3, -7, -2, -5);
    ivec4 b = ivec4(-2, -1, -3, -1);
    return a + b;
}

// run: test_ivec4_add_negative_negative() == ivec4(-5, -8, -5, -6)

ivec4 test_ivec4_add_zero() {
    ivec4 a = ivec4(42, 17, 23, 8);
    ivec4 b = ivec4(0, 0, 0, 0);
    return a + b;
}

// run: test_ivec4_add_zero() == ivec4(42, 17, 23, 8)

ivec4 test_ivec4_add_variables() {
    ivec4 a = ivec4(15, 10, 5, 12);
    ivec4 b = ivec4(27, 5, 12, 3);
    return a + b;
}

// run: test_ivec4_add_variables() == ivec4(42, 15, 17, 15)

ivec4 test_ivec4_add_expressions() {
    return ivec4(8, 4, 6, 2) + ivec4(6, 2, 3, 4);
}

// run: test_ivec4_add_expressions() == ivec4(14, 6, 9, 6)

ivec4 test_ivec4_add_in_assignment() {
    ivec4 result = ivec4(5, 3, 2, 1);
    result = result + ivec4(10, 7, 8, 9);
    return result;
}

// run: test_ivec4_add_in_assignment() == ivec4(15, 10, 10, 10)

ivec4 test_ivec4_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)
    // Addition saturates to max for each component
    ivec4 a = ivec4(100000, 50000, 25000, 10000);
    ivec4 b = ivec4(200000, 30000, 15000, 5000);
    return a + b;
}

// run: test_ivec4_add_large_numbers() == ivec4(300000, 80000, 40000, 15000)

ivec4 test_ivec4_add_mixed_components() {
    ivec4 a = ivec4(1, -2, 3, -4);
    ivec4 b = ivec4(-3, 4, -1, 2);
    return a + b;
}

// run: test_ivec4_add_mixed_components() == ivec4(-2, 2, 2, -2)

