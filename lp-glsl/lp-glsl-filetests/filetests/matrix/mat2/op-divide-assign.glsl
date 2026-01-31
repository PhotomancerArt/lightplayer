// test run
// target riscv32.q32

// ============================================================================
// Divide Assign: mat2 /= mat2 (component-wise division) or mat2 /= float (component-wise)
// ============================================================================

mat2 test_mat2_divide_assign_matrix() {
    mat2 result = mat2(6.0, 8.0, 10.0, 12.0);
    result /= mat2(2.0, 4.0, 5.0, 6.0);
    return result;
}

// run: test_mat2_divide_assign_matrix() ~= mat2(3.0, 2.0, 2.0, 2.0) [expect-fail]

mat2 test_mat2_divide_assign_matrix_identity() {
    mat2 result = mat2(2.0, 4.0, 6.0, 8.0);
    result /= mat2(1.0, 1.0, 1.0, 1.0); // divide by identity
    return result;
}

// run: test_mat2_divide_assign_matrix_identity() ~= mat2(2.0, 4.0, 6.0, 8.0) [expect-fail]

mat2 test_mat2_divide_assign_matrix_variables() {
    mat2 a = mat2(15.0, 20.0, 25.0, 30.0);
    mat2 b = mat2(3.0, 4.0, 5.0, 6.0);
    a /= b;
    return a;
}

// run: test_mat2_divide_assign_matrix_variables() ~= mat2(5.0, 5.0, 5.0, 5.0) [expect-fail]

mat2 test_mat2_divide_assign_matrix_expressions() {
    mat2 result = mat2(8.0, 12.0, 16.0, 20.0);
    result /= mat2(2.0, 3.0, 4.0, 5.0) / mat2(2.0, 3.0, 4.0, 5.0); // divide by 1.0 matrix
    return result;
}

// run: test_mat2_divide_assign_matrix_expressions() ~= mat2(8.0, 12.0, 16.0, 20.0) [expect-fail]

mat2 test_mat2_divide_assign_scalar() {
    mat2 result = mat2(10.0, 20.0, 30.0, 40.0);
    result /= 2.0;
    return result;
}

// run: test_mat2_divide_assign_scalar() ~= mat2(5.0, 10.0, 15.0, 20.0) [expect-fail]

mat2 test_mat2_divide_assign_scalar_one() {
    mat2 result = mat2(5.0, 10.0, 15.0, 20.0);
    result /= 1.0;
    return result;
}

// run: test_mat2_divide_assign_scalar_one() ~= mat2(5.0, 10.0, 15.0, 20.0) [expect-fail]

mat2 test_mat2_divide_assign_scalar_variables() {
    mat2 a = mat2(12.0, 18.0, 24.0, 30.0);
    float s = 3.0;
    a /= s;
    return a;
}

// run: test_mat2_divide_assign_scalar_variables() ~= mat2(4.0, 6.0, 8.0, 10.0) [expect-fail]

mat2 test_mat2_divide_assign_scalar_expressions() {
    mat2 result = mat2(16.0, 24.0, 32.0, 40.0);
    result /= 4.0 * 2.0; // divide by 8.0
    return result;
}

// run: test_mat2_divide_assign_scalar_expressions() ~= mat2(2.0, 3.0, 4.0, 5.0) [expect-fail]

mat2 test_mat2_divide_assign_scalar_negative() {
    mat2 result = mat2(10.0, 20.0, 30.0, 40.0);
    result /= -2.0;
    return result;
}

// run: test_mat2_divide_assign_scalar_negative() ~= mat2(-5.0, -10.0, -15.0, -20.0) [expect-fail]

mat2 test_mat2_divide_assign_scalar_fractional() {
    mat2 result = mat2(10.0, 20.0, 30.0, 40.0);
    result /= 0.5;
    return result;
}

// run: test_mat2_divide_assign_scalar_fractional() ~= mat2(20.0, 40.0, 60.0, 80.0) [expect-fail]




