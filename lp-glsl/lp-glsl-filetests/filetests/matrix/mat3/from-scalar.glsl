// test run
// target riscv32.q32

// ============================================================================
// From Scalar: mat3(float) - creates diagonal matrix
// ============================================================================

mat3 test_mat3_from_scalar_positive() {
    // Constructor mat3(float) creates diagonal matrix with float on diagonal
    return mat3(5.0);
}

// run: test_mat3_from_scalar_positive() ~= mat3(5.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 5.0)

mat3 test_mat3_from_scalar_negative() {
    return mat3(-3.0);
}

// run: test_mat3_from_scalar_negative() ~= mat3(-3.0, 0.0, 0.0, 0.0, -3.0, 0.0, 0.0, 0.0, -3.0)

mat3 test_mat3_from_scalar_zero() {
    return mat3(0.0);
}

// run: test_mat3_from_scalar_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_mat3_from_scalar_one() {
    return mat3(1.0);
}

// run: test_mat3_from_scalar_one() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_from_scalar_variable() {
    float x = 2.5;
    return mat3(x);
}

// run: test_mat3_from_scalar_variable() ~= mat3(2.5, 0.0, 0.0, 0.0, 2.5, 0.0, 0.0, 0.0, 2.5)

mat3 test_mat3_from_scalar_expression() {
    return mat3(3.0 + 2.0);
}

// run: test_mat3_from_scalar_expression() ~= mat3(5.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 5.0)

mat3 test_mat3_from_scalar_function_result() {
    return mat3(float(7)); // int to float conversion
}

// run: test_mat3_from_scalar_function_result() ~= mat3(7.0, 0.0, 0.0, 0.0, 7.0, 0.0, 0.0, 0.0, 7.0)

mat3 test_mat3_from_scalar_in_assignment() {
    mat3 result;
    result = mat3(4.0);
    return result;
}

// run: test_mat3_from_scalar_in_assignment() ~= mat3(4.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 4.0)

mat3 test_mat3_from_scalar_large_value() {
    return mat3(1000.0);
}

// run: test_mat3_from_scalar_large_value() ~= mat3(1000.0, 0.0, 0.0, 0.0, 1000.0, 0.0, 0.0, 0.0, 1000.0)

mat3 test_mat3_from_scalar_fractional() {
    return mat3(0.5);
}

// run: test_mat3_from_scalar_fractional() ~= mat3(0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5)
