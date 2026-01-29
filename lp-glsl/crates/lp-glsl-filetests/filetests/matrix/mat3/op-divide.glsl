// test run
// target riscv32.q32

// ============================================================================
// Divide: mat3 / mat3 -> mat3 (component-wise)
// ============================================================================

mat3 test_mat3_divide_simple() {
    // Division with matrices (component-wise)
    mat3 a = mat3(6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0);
    mat3 b = mat3(2.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    return a / b;
}

// run: test_mat3_divide_simple() ~= mat3(3.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0) [expect-fail]

mat3 test_mat3_divide_identity() {
    mat3 a = mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0);
    mat3 b = mat3(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0); // identity for division
    return a / b;
}

// run: test_mat3_divide_identity() ~= mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0) [expect-fail]

mat3 test_mat3_divide_variables() {
    mat3 a = mat3(15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 55.0);
    mat3 b = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    return a / b;
}

// run: test_mat3_divide_variables() ~= mat3(5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0) [expect-fail]

mat3 test_mat3_divide_expressions() {
    return mat3(8.0, 12.0, 16.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0) / mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0);
}

// run: test_mat3_divide_expressions() ~= mat3(4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0) [expect-fail]

mat3 test_mat3_divide_in_assignment() {
    mat3 result = mat3(10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0);
    result = result / mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0);
    return result;
}

// run: test_mat3_divide_in_assignment() ~= mat3(5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0) [expect-fail]

mat3 test_mat3_divide_scalar() {
    mat3 a = mat3(10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0);
    return a / 2.0;
}

// run: test_mat3_divide_scalar() ~= mat3(5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0) [expect-fail]

mat3 test_mat3_divide_scalar_variables() {
    mat3 a = mat3(12.0, 18.0, 24.0, 30.0, 36.0, 42.0, 48.0, 54.0, 60.0);
    float s = 3.0;
    return a / s;
}

// run: test_mat3_divide_scalar_variables() ~= mat3(4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0) [expect-fail]

mat3 test_mat3_divide_scalar_expressions() {
    return mat3(16.0, 24.0, 32.0, 40.0, 48.0, 56.0, 64.0, 72.0, 80.0) / (4.0 * 2.0);
}

// run: test_mat3_divide_scalar_expressions() ~= mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0) [expect-fail]

mat3 test_mat3_divide_scalar_assignment() {
    mat3 result = mat3(14.0, 21.0, 28.0, 35.0, 42.0, 49.0, 56.0, 63.0, 70.0);
    result = result / 7.0;
    return result;
}

// run: test_mat3_divide_scalar_assignment() ~= mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0) [expect-fail]




