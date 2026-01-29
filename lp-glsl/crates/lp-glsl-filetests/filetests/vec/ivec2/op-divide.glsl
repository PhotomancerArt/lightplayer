// test run
// target riscv32.q32

// ============================================================================
// Divide: ivec2 / ivec2 -> ivec2 (component-wise, truncates toward zero)
// ============================================================================

ivec2 test_ivec2_divide_positive_positive() {
    // Division with positive vectors (component-wise, truncates toward zero)
    ivec2 a = ivec2(10, 15);
    ivec2 b = ivec2(3, 4);
    return a / b;
}

// run: test_ivec2_divide_positive_positive() == ivec2(3, 3)

ivec2 test_ivec2_divide_positive_negative() {
    ivec2 a = ivec2(10, 15);
    ivec2 b = ivec2(-3, -4);
    return a / b;
}

// run: test_ivec2_divide_positive_negative() == ivec2(-3, -3)

ivec2 test_ivec2_divide_negative_negative() {
    ivec2 a = ivec2(-10, -15);
    ivec2 b = ivec2(-3, -4);
    return a / b;
}

// run: test_ivec2_divide_negative_negative() == ivec2(3, 3)

ivec2 test_ivec2_divide_by_one() {
    ivec2 a = ivec2(42, 17);
    ivec2 b = ivec2(1, 1);
    return a / b;
}

// run: test_ivec2_divide_by_one() == ivec2(42, 17)

ivec2 test_ivec2_divide_variables() {
    ivec2 a = ivec2(20, 30);
    ivec2 b = ivec2(4, 6);
    return a / b;
}

// run: test_ivec2_divide_variables() == ivec2(5, 5)

ivec2 test_ivec2_divide_expressions() {
    return ivec2(24, 18) / ivec2(3, 6);
}

// run: test_ivec2_divide_expressions() == ivec2(8, 3)

ivec2 test_ivec2_divide_in_assignment() {
    ivec2 result = ivec2(15, 20);
    result = result / ivec2(3, 4);
    return result;
}

// run: test_ivec2_divide_in_assignment() == ivec2(5, 5)

ivec2 test_ivec2_divide_remainder() {
    ivec2 a = ivec2(17, 19);
    ivec2 b = ivec2(5, 7);
    return a / b;
}

// run: test_ivec2_divide_remainder() == ivec2(3, 2)

ivec2 test_ivec2_divide_mixed_components() {
    ivec2 a = ivec2(10, -15);
    ivec2 b = ivec2(3, -5);
    return a / b;
}

// run: test_ivec2_divide_mixed_components() == ivec2(3, 3)
