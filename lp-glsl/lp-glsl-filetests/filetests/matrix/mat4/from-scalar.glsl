// test run
// target riscv32.q32

// ============================================================================
// From Scalar: mat4(float) - creates diagonal matrix
// ============================================================================

mat4 test_mat4_from_scalar_positive() {
    // Constructor mat4(float) creates diagonal matrix with float on diagonal
    return mat4(5.0);
}

// run: test_mat4_from_scalar_positive() ~= mat4(5.0, 0.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 5.0)

mat4 test_mat4_from_scalar_negative() {
    return mat4(-3.0);
}

// run: test_mat4_from_scalar_negative() ~= mat4(-3.0, 0.0, 0.0, 0.0, 0.0, -3.0, 0.0, 0.0, 0.0, 0.0, -3.0, 0.0, 0.0, 0.0, 0.0, -3.0)

mat4 test_mat4_from_scalar_zero() {
    return mat4(0.0);
}

// run: test_mat4_from_scalar_zero() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat4 test_mat4_from_scalar_one() {
    return mat4(1.0);
}

// run: test_mat4_from_scalar_one() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_from_scalar_variable() {
    float x = 2.5;
    return mat4(x);
}

// run: test_mat4_from_scalar_variable() ~= mat4(2.5, 0.0, 0.0, 0.0, 0.0, 2.5, 0.0, 0.0, 0.0, 0.0, 2.5, 0.0, 0.0, 0.0, 0.0, 2.5)

mat4 test_mat4_from_scalar_expression() {
    return mat4(3.0 + 2.0);
}

// run: test_mat4_from_scalar_expression() ~= mat4(5.0, 0.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 5.0)

mat4 test_mat4_from_scalar_function_result() {
    return mat4(float(7)); // int to float conversion
}

// run: test_mat4_from_scalar_function_result() ~= mat4(7.0, 0.0, 0.0, 0.0, 0.0, 7.0, 0.0, 0.0, 0.0, 0.0, 7.0, 0.0, 0.0, 0.0, 0.0, 7.0)

mat4 test_mat4_from_scalar_in_assignment() {
    mat4 result;
    result = mat4(4.0);
    return result;
}

// run: test_mat4_from_scalar_in_assignment() ~= mat4(4.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 4.0)

mat4 test_mat4_from_scalar_large_value() {
    return mat4(1000000.0);
}

// run: test_mat4_from_scalar_large_value() ~= mat4(32767.0, 0.0, 0.0, 0.0, 0.0, 32767.0, 0.0, 0.0, 0.0, 0.0, 32767.0, 0.0, 0.0, 0.0, 0.0, 32767.0) [expect-fail]

mat4 test_mat4_from_scalar_fractional() {
    return mat4(0.5);
}

// run: test_mat4_from_scalar_fractional() ~= mat4(0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5)
