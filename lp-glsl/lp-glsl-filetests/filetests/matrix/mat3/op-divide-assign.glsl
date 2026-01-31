// test run
// target riscv32.q32

// ============================================================================
// Divide Assign: mat3 /= mat3 (component-wise division) or mat3 /= float (component-wise)
// ============================================================================

mat3 test_mat3_divide_assign_matrix() {
    mat3 result = mat3(6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0);
    result /= mat3(2.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    return result;
}

// run: test_mat3_divide_assign_matrix() ~= mat3(3.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0) [expect-fail]

mat3 test_mat3_divide_assign_matrix_identity() {
    mat3 result = mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0);
    result /= mat3(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0); // divide by identity
    return result;
}

// run: test_mat3_divide_assign_matrix_identity() ~= mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0) [expect-fail]

mat3 test_mat3_divide_assign_matrix_variables() {
    mat3 a = mat3(15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 55.0);
    mat3 b = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    a /= b;
    return a;
}

// run: test_mat3_divide_assign_matrix_variables() ~= mat3(5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0) [expect-fail]

mat3 test_mat3_divide_assign_matrix_expressions() {
    mat3 result = mat3(8.0, 12.0, 16.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0);
    result /= mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0) / mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0); // divide by 1.0 matrix
    return result;
}

// run: test_mat3_divide_assign_matrix_expressions() ~= mat3(8.0, 12.0, 16.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0) [expect-fail]

mat3 test_mat3_divide_assign_scalar() {
    mat3 result = mat3(10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0);
    result /= 2.0;
    return result;
}

// run: test_mat3_divide_assign_scalar() ~= mat3(5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0) [expect-fail]

mat3 test_mat3_divide_assign_scalar_one() {
    mat3 result = mat3(5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0);
    result /= 1.0;
    return result;
}

// run: test_mat3_divide_assign_scalar_one() ~= mat3(5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0) [expect-fail]

mat3 test_mat3_divide_assign_scalar_variables() {
    mat3 a = mat3(12.0, 18.0, 24.0, 30.0, 36.0, 42.0, 48.0, 54.0, 60.0);
    float s = 3.0;
    a /= s;
    return a;
}

// run: test_mat3_divide_assign_scalar_variables() ~= mat3(4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0) [expect-fail]

mat3 test_mat3_divide_assign_scalar_expressions() {
    mat3 result = mat3(16.0, 24.0, 32.0, 40.0, 48.0, 56.0, 64.0, 72.0, 80.0);
    result /= 4.0 * 2.0; // divide by 8.0
    return result;
}

// run: test_mat3_divide_assign_scalar_expressions() ~= mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0) [expect-fail]

mat3 test_mat3_divide_assign_scalar_negative() {
    mat3 result = mat3(10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0);
    result /= -2.0;
    return result;
}

// run: test_mat3_divide_assign_scalar_negative() ~= mat3(-5.0, -10.0, -15.0, -20.0, -25.0, -30.0, -35.0, -40.0, -45.0) [expect-fail]

mat3 test_mat3_divide_assign_scalar_fractional() {
    mat3 result = mat3(10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0);
    result /= 0.5;
    return result;
}

// run: test_mat3_divide_assign_scalar_fractional() ~= mat3(20.0, 40.0, 60.0, 80.0, 100.0, 120.0, 140.0, 160.0, 180.0) [expect-fail]




