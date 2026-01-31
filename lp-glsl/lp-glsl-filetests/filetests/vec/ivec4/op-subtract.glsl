// test run
// target riscv32.q32

// ============================================================================
// Subtract: ivec4 - ivec4 -> ivec4 (component-wise)
// ============================================================================

ivec4 test_ivec4_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    ivec4 a = ivec4(10, 8, 5, 7);
    ivec4 b = ivec4(3, 2, 1, 4);
    return a - b;
}

// run: test_ivec4_subtract_positive_positive() == ivec4(7, 6, 4, 3)

ivec4 test_ivec4_subtract_positive_negative() {
    ivec4 a = ivec4(10, 8, 5, 7);
    ivec4 b = ivec4(-4, -2, -1, -3);
    return a - b;
}

// run: test_ivec4_subtract_positive_negative() == ivec4(14, 10, 6, 10)

ivec4 test_ivec4_subtract_negative_negative() {
    ivec4 a = ivec4(-3, -7, -2, -5);
    ivec4 b = ivec4(-2, -1, -3, -1);
    return a - b;
}

// run: test_ivec4_subtract_negative_negative() == ivec4(-1, -6, 1, -4)

ivec4 test_ivec4_subtract_zero() {
    ivec4 a = ivec4(42, 17, 23, 8);
    ivec4 b = ivec4(0, 0, 0, 0);
    return a - b;
}

// run: test_ivec4_subtract_zero() == ivec4(42, 17, 23, 8)

ivec4 test_ivec4_subtract_variables() {
    ivec4 a = ivec4(50, 20, 15, 25);
    ivec4 b = ivec4(15, 5, 3, 10);
    return a - b;
}

// run: test_ivec4_subtract_variables() == ivec4(35, 15, 12, 15)

ivec4 test_ivec4_subtract_expressions() {
    return ivec4(20, 10, 8, 12) - ivec4(5, 3, 2, 7);
}

// run: test_ivec4_subtract_expressions() == ivec4(15, 7, 6, 5)

ivec4 test_ivec4_subtract_in_assignment() {
    ivec4 result = ivec4(20, 15, 10, 18);
    result = result - ivec4(8, 5, 3, 9);
    return result;
}

// run: test_ivec4_subtract_in_assignment() == ivec4(12, 10, 7, 9)

ivec4 test_ivec4_subtract_large_numbers() {
    ivec4 a = ivec4(500000, 300000, 200000, 100000);
    ivec4 b = ivec4(200000, 100000, 50000, 25000);
    return a - b;
}

// run: test_ivec4_subtract_large_numbers() == ivec4(300000, 200000, 150000, 75000)

ivec4 test_ivec4_subtract_mixed_components() {
    ivec4 a = ivec4(5, -2, 8, -3);
    ivec4 b = ivec4(3, -4, 2, -1);
    return a - b;
}

// run: test_ivec4_subtract_mixed_components() == ivec4(2, 2, 6, -2)
