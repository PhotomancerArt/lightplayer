// test run
// target riscv32.q32

// ============================================================================
// Subtract: ivec3 - ivec3 -> ivec3 (component-wise)
// ============================================================================

ivec3 test_ivec3_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    ivec3 a = ivec3(10, 8, 5);
    ivec3 b = ivec3(3, 2, 1);
    return a - b;
}

// run: test_ivec3_subtract_positive_positive() == ivec3(7, 6, 4)

ivec3 test_ivec3_subtract_positive_negative() {
    ivec3 a = ivec3(10, 8, 5);
    ivec3 b = ivec3(-4, -2, -1);
    return a - b;
}

// run: test_ivec3_subtract_positive_negative() == ivec3(14, 10, 6)

ivec3 test_ivec3_subtract_negative_negative() {
    ivec3 a = ivec3(-3, -7, -2);
    ivec3 b = ivec3(-2, -1, -3);
    return a - b;
}

// run: test_ivec3_subtract_negative_negative() == ivec3(-1, -6, 1)

ivec3 test_ivec3_subtract_zero() {
    ivec3 a = ivec3(42, 17, 23);
    ivec3 b = ivec3(0, 0, 0);
    return a - b;
}

// run: test_ivec3_subtract_zero() == ivec3(42, 17, 23)

ivec3 test_ivec3_subtract_variables() {
    ivec3 a = ivec3(50, 20, 15);
    ivec3 b = ivec3(15, 5, 3);
    return a - b;
}

// run: test_ivec3_subtract_variables() == ivec3(35, 15, 12)

ivec3 test_ivec3_subtract_expressions() {
    return ivec3(20, 10, 8) - ivec3(5, 3, 2);
}

// run: test_ivec3_subtract_expressions() == ivec3(15, 7, 6)

ivec3 test_ivec3_subtract_in_assignment() {
    ivec3 result = ivec3(20, 15, 10);
    result = result - ivec3(8, 5, 3);
    return result;
}

// run: test_ivec3_subtract_in_assignment() == ivec3(12, 10, 7)

ivec3 test_ivec3_subtract_large_numbers() {
    ivec3 a = ivec3(500000, 300000, 200000);
    ivec3 b = ivec3(200000, 100000, 50000);
    return a - b;
}

// run: test_ivec3_subtract_large_numbers() == ivec3(300000, 200000, 150000)

ivec3 test_ivec3_subtract_mixed_components() {
    ivec3 a = ivec3(5, -2, 8);
    ivec3 b = ivec3(3, -4, 2);
    return a - b;
}

// run: test_ivec3_subtract_mixed_components() == ivec3(2, 2, 6)
