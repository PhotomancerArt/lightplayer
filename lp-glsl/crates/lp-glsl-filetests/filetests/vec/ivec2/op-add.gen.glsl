// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec2/op-add --write
//
// test run
// target riscv32.q32

// ============================================================================
// Add: ivec2 + ivec2 -> ivec2 (component-wise)
// ============================================================================

ivec2 test_ivec2_add_positive_positive() {
    // Addition with positive vectors (component-wise)
    ivec2 a = ivec2(5, 3);
    ivec2 b = ivec2(2, 4);
    return a + b;
}

// run: test_ivec2_add_positive_positive() == ivec2(7, 7)

ivec2 test_ivec2_add_positive_negative() {
    ivec2 a = ivec2(10, 8);
    ivec2 b = ivec2(-4, -2);
    return a + b;
}

// run: test_ivec2_add_positive_negative() == ivec2(6, 6)

ivec2 test_ivec2_add_negative_negative() {
    ivec2 a = ivec2(-3, -7);
    ivec2 b = ivec2(-2, -1);
    return a + b;
}

// run: test_ivec2_add_negative_negative() == ivec2(-5, -8)

ivec2 test_ivec2_add_zero() {
    ivec2 a = ivec2(42, 17);
    ivec2 b = ivec2(0, 0);
    return a + b;
}

// run: test_ivec2_add_zero() == ivec2(42, 17)

ivec2 test_ivec2_add_variables() {
    ivec2 a = ivec2(15, 10);
    ivec2 b = ivec2(27, 5);
    return a + b;
}

// run: test_ivec2_add_variables() == ivec2(42, 15)

ivec2 test_ivec2_add_expressions() {
    return ivec2(8, 4) + ivec2(6, 2);
}

// run: test_ivec2_add_expressions() == ivec2(14, 6)

ivec2 test_ivec2_add_in_assignment() {
    ivec2 result = ivec2(5, 3);
    result = result + ivec2(10, 7);
    return result;
}

// run: test_ivec2_add_in_assignment() == ivec2(15, 10)

ivec2 test_ivec2_add_large_numbers() {
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)
    // Addition saturates to max for each component
    ivec2 a = ivec2(100000, 50000);
    ivec2 b = ivec2(200000, 30000);
    return a + b;
}

// run: test_ivec2_add_large_numbers() == ivec2(300000, 80000)

ivec2 test_ivec2_add_mixed_components() {
    ivec2 a = ivec2(1, -2);
    ivec2 b = ivec2(-3, 4);
    return a + b;
}

// run: test_ivec2_add_mixed_components() == ivec2(-2, 2)

