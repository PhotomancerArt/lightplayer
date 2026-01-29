// test run
// target riscv32.q32

// ============================================================================
// Subtract: ivec2 - ivec2 -> ivec2 (component-wise)
// ============================================================================

ivec2 test_ivec2_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    ivec2 a = ivec2(10, 8);
    ivec2 b = ivec2(3, 2);
    return a - b;
}

// run: test_ivec2_subtract_positive_positive() == ivec2(7, 6)

ivec2 test_ivec2_subtract_positive_negative() {
    ivec2 a = ivec2(10, 8);
    ivec2 b = ivec2(-4, -2);
    return a - b;
}

// run: test_ivec2_subtract_positive_negative() == ivec2(14, 10)

ivec2 test_ivec2_subtract_negative_negative() {
    ivec2 a = ivec2(-3, -7);
    ivec2 b = ivec2(-2, -1);
    return a - b;
}

// run: test_ivec2_subtract_negative_negative() == ivec2(-1, -6)

ivec2 test_ivec2_subtract_zero() {
    ivec2 a = ivec2(42, 17);
    ivec2 b = ivec2(0, 0);
    return a - b;
}

// run: test_ivec2_subtract_zero() == ivec2(42, 17)

ivec2 test_ivec2_subtract_variables() {
    ivec2 a = ivec2(50, 20);
    ivec2 b = ivec2(15, 5);
    return a - b;
}

// run: test_ivec2_subtract_variables() == ivec2(35, 15)

ivec2 test_ivec2_subtract_expressions() {
    return ivec2(20, 10) - ivec2(5, 3);
}

// run: test_ivec2_subtract_expressions() == ivec2(15, 7)

ivec2 test_ivec2_subtract_in_assignment() {
    ivec2 result = ivec2(20, 15);
    result = result - ivec2(8, 5);
    return result;
}

// run: test_ivec2_subtract_in_assignment() == ivec2(12, 10)

ivec2 test_ivec2_subtract_large_numbers() {
    ivec2 a = ivec2(500000, 300000);
    ivec2 b = ivec2(200000, 100000);
    return a - b;
}

// run: test_ivec2_subtract_large_numbers() == ivec2(300000, 200000)

ivec2 test_ivec2_subtract_mixed_components() {
    ivec2 a = ivec2(5, -2);
    ivec2 b = ivec2(3, -4);
    return a - b;
}

// run: test_ivec2_subtract_mixed_components() == ivec2(2, 2)
