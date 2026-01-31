// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec3/op-add --write
//
// test run
// target riscv32.q32

// ============================================================================
// Add: ivec3 + ivec3 -> ivec3 (component-wise)
// ============================================================================

ivec3 test_ivec3_add_positive_positive() {
    // Addition with positive vectors (component-wise)
    ivec3 a = ivec3(5, 3, 2);
    ivec3 b = ivec3(2, 4, 1);
    return a + b;
}

// run: test_ivec3_add_positive_positive() == ivec3(7, 7, 3)

ivec3 test_ivec3_add_positive_negative() {
    ivec3 a = ivec3(10, 8, 5);
    ivec3 b = ivec3(-4, -2, -1);
    return a + b;
}

// run: test_ivec3_add_positive_negative() == ivec3(6, 6, 4)

ivec3 test_ivec3_add_negative_negative() {
    ivec3 a = ivec3(-3, -7, -2);
    ivec3 b = ivec3(-2, -1, -3);
    return a + b;
}

// run: test_ivec3_add_negative_negative() == ivec3(-5, -8, -5)

ivec3 test_ivec3_add_zero() {
    ivec3 a = ivec3(42, 17, 23);
    ivec3 b = ivec3(0, 0, 0);
    return a + b;
}

// run: test_ivec3_add_zero() == ivec3(42, 17, 23)

ivec3 test_ivec3_add_variables() {
    ivec3 a = ivec3(15, 10, 5);
    ivec3 b = ivec3(27, 5, 12);
    return a + b;
}

// run: test_ivec3_add_variables() == ivec3(42, 15, 17)

ivec3 test_ivec3_add_expressions() {
    return ivec3(8, 4, 6) + ivec3(6, 2, 3);
}

// run: test_ivec3_add_expressions() == ivec3(14, 6, 9)

ivec3 test_ivec3_add_in_assignment() {
    ivec3 result = ivec3(5, 3, 2);
    result = result + ivec3(10, 7, 8);
    return result;
}

// run: test_ivec3_add_in_assignment() == ivec3(15, 10, 10)

ivec3 test_ivec3_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)
    // Addition saturates to max for each component
    ivec3 a = ivec3(100000, 50000, 25000);
    ivec3 b = ivec3(200000, 30000, 15000);
    return a + b;
}

// run: test_ivec3_add_large_numbers() == ivec3(300000, 80000, 40000)

ivec3 test_ivec3_add_mixed_components() {
    ivec3 a = ivec3(1, -2, 3);
    ivec3 b = ivec3(-3, 4, -1);
    return a + b;
}

// run: test_ivec3_add_mixed_components() == ivec3(-2, 2, 2)

