// test run
// target riscv32.q32

// ============================================================================
// From Scalar: mat2(float) - creates diagonal matrix
// ============================================================================

mat2 test_mat2_from_scalar_positive() {
    // Constructor mat2(float) creates diagonal matrix with float on diagonal
    return mat2(5.0);
}

// run: test_mat2_from_scalar_positive() ~= mat2(5.0, 0.0, 0.0, 5.0)

mat2 test_mat2_from_scalar_negative() {
    return mat2(-3.0);
}

// run: test_mat2_from_scalar_negative() ~= mat2(-3.0, 0.0, 0.0, -3.0)

mat2 test_mat2_from_scalar_zero() {
    return mat2(0.0);
}

// run: test_mat2_from_scalar_zero() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_from_scalar_one() {
    return mat2(1.0);
}

// run: test_mat2_from_scalar_one() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_mat2_from_scalar_variable() {
    float x = 2.5;
    return mat2(x);
}

// run: test_mat2_from_scalar_variable() ~= mat2(2.5, 0.0, 0.0, 2.5)

mat2 test_mat2_from_scalar_expression() {
    return mat2(3.0 + 2.0);
}

// run: test_mat2_from_scalar_expression() ~= mat2(5.0, 0.0, 0.0, 5.0)

mat2 test_mat2_from_scalar_function_result() {
    return mat2(float(7)); // int to float conversion
}

// run: test_mat2_from_scalar_function_result() ~= mat2(7.0, 0.0, 0.0, 7.0)

mat2 test_mat2_from_scalar_in_assignment() {
    mat2 result;
    result = mat2(4.0);
    return result;
}

// run: test_mat2_from_scalar_in_assignment() ~= mat2(4.0, 0.0, 0.0, 4.0)

mat2 test_mat2_from_scalar_large_value() {
    return mat2(1000.0);
}

// run: test_mat2_from_scalar_large_value() ~= mat2(1000.0, 0.0, 0.0, 1000.0)

mat2 test_mat2_from_scalar_fractional() {
    return mat2(0.5);
}

// run: test_mat2_from_scalar_fractional() ~= mat2(0.5, 0.0, 0.0, 0.5)
