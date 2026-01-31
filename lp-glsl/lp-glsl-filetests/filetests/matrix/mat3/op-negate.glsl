// test run
// target riscv32.q32

// ============================================================================
// Negate: -mat3 -> mat3 (unary negation)
// ============================================================================

mat3 test_mat3_negate_positive() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return -a;
}

// run: test_mat3_negate_positive() ~= mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0)

mat3 test_mat3_negate_negative() {
    mat3 a = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    return -a;
}

// run: test_mat3_negate_negative() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_negate_zero() {
    mat3 a = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return -a;
}

// run: test_mat3_negate_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_mat3_negate_mixed() {
    mat3 a = mat3(1.0, -2.0, 3.0, -4.0, 5.0, -6.0, 7.0, -8.0, 9.0);
    return -a;
}

// run: test_mat3_negate_mixed() ~= mat3(-1.0, 2.0, -3.0, 4.0, -5.0, 6.0, -7.0, 8.0, -9.0)

mat3 test_mat3_negate_variables() {
    mat3 a = mat3(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0);
    return -a;
}

// run: test_mat3_negate_variables() ~= mat3(-5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, -13.0)

mat3 test_mat3_negate_expressions() {
    return -mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0);
}

// run: test_mat3_negate_expressions() ~= mat3(-2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0)

mat3 test_mat3_negate_in_expression() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0);
    return -a + b;
}

// run: test_mat3_negate_in_expression() ~= mat3(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0)

mat3 test_mat3_negate_assignment() {
    mat3 result = mat3(10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0);
    result = -result;
    return result;
}

// run: test_mat3_negate_assignment() ~= mat3(-10.0, -20.0, -30.0, -40.0, -50.0, -60.0, -70.0, -80.0, -90.0)

mat3 test_mat3_negate_fractional() {
    mat3 a = mat3(1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5);
    return -a;
}

// run: test_mat3_negate_fractional() ~= mat3(-1.5, -2.5, -3.5, -4.5, -5.5, -6.5, -7.5, -8.5, -9.5)




