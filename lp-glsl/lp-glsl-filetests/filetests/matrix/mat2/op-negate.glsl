// test run
// target riscv32.q32

// ============================================================================
// Negate: -mat2 -> mat2 (unary negation)
// ============================================================================

mat2 test_mat2_negate_positive() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    return -a;
}

// run: test_mat2_negate_positive() ~= mat2(-1.0, -2.0, -3.0, -4.0)

mat2 test_mat2_negate_negative() {
    mat2 a = mat2(-1.0, -2.0, -3.0, -4.0);
    return -a;
}

// run: test_mat2_negate_negative() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_negate_zero() {
    mat2 a = mat2(0.0, 0.0, 0.0, 0.0);
    return -a;
}

// run: test_mat2_negate_zero() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_negate_mixed() {
    mat2 a = mat2(1.0, -2.0, 3.0, -4.0);
    return -a;
}

// run: test_mat2_negate_mixed() ~= mat2(-1.0, 2.0, -3.0, 4.0)

mat2 test_mat2_negate_variables() {
    mat2 a = mat2(5.0, 6.0, 7.0, 8.0);
    return -a;
}

// run: test_mat2_negate_variables() ~= mat2(-5.0, -6.0, -7.0, -8.0)

mat2 test_mat2_negate_expressions() {
    return -mat2(2.0, 3.0, 4.0, 5.0);
}

// run: test_mat2_negate_expressions() ~= mat2(-2.0, -3.0, -4.0, -5.0)

mat2 test_mat2_negate_in_expression() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(2.0, 3.0, 4.0, 5.0);
    return -a + b;
}

// run: test_mat2_negate_in_expression() ~= mat2(1.0, 1.0, 1.0, 1.0)

mat2 test_mat2_negate_assignment() {
    mat2 result = mat2(10.0, 20.0, 30.0, 40.0);
    result = -result;
    return result;
}

// run: test_mat2_negate_assignment() ~= mat2(-10.0, -20.0, -30.0, -40.0)

mat2 test_mat2_negate_fractional() {
    mat2 a = mat2(1.5, 2.5, 3.5, 4.5);
    return -a;
}

// run: test_mat2_negate_fractional() ~= mat2(-1.5, -2.5, -3.5, -4.5)




