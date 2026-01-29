// test run
// target riscv32.q32

// ============================================================================
// Divide: ivec3 / ivec3 -> ivec3 (component-wise, truncates toward zero)
// ============================================================================

ivec3 test_ivec3_divide_positive_positive() {
    // Division with positive vectors (component-wise, truncates toward zero)
    ivec3 a = ivec3(10, 15, 8);
    ivec3 b = ivec3(3, 4, 2);
    return a / b;
}

// run: test_ivec3_divide_positive_positive() == ivec3(3, 3, 4)

ivec3 test_ivec3_divide_positive_negative() {
    ivec3 a = ivec3(10, 15, 8);
    ivec3 b = ivec3(-3, -4, -2);
    return a / b;
}

// run: test_ivec3_divide_positive_negative() == ivec3(-3, -3, -4)

ivec3 test_ivec3_divide_negative_negative() {
    ivec3 a = ivec3(-10, -15, -8);
    ivec3 b = ivec3(-3, -4, -2);
    return a / b;
}

// run: test_ivec3_divide_negative_negative() == ivec3(3, 3, 4)

ivec3 test_ivec3_divide_by_one() {
    ivec3 a = ivec3(42, 17, 23);
    ivec3 b = ivec3(1, 1, 1);
    return a / b;
}

// run: test_ivec3_divide_by_one() == ivec3(42, 17, 23)

ivec3 test_ivec3_divide_variables() {
    ivec3 a = ivec3(20, 30, 24);
    ivec3 b = ivec3(4, 6, 3);
    return a / b;
}

// run: test_ivec3_divide_variables() == ivec3(5, 5, 8)

ivec3 test_ivec3_divide_expressions() {
    return ivec3(24, 18, 30) / ivec3(3, 6, 5);
}

// run: test_ivec3_divide_expressions() == ivec3(8, 3, 6)

ivec3 test_ivec3_divide_in_assignment() {
    ivec3 result = ivec3(15, 20, 25);
    result = result / ivec3(3, 4, 5);
    return result;
}

// run: test_ivec3_divide_in_assignment() == ivec3(5, 5, 5)

ivec3 test_ivec3_divide_remainder() {
    ivec3 a = ivec3(17, 19, 23);
    ivec3 b = ivec3(5, 7, 6);
    return a / b;
}

// run: test_ivec3_divide_remainder() == ivec3(3, 2, 3)

ivec3 test_ivec3_divide_mixed_components() {
    ivec3 a = ivec3(10, -15, 8);
    ivec3 b = ivec3(3, -5, 2);
    return a / b;
}

// run: test_ivec3_divide_mixed_components() == ivec3(3, 3, 4)
