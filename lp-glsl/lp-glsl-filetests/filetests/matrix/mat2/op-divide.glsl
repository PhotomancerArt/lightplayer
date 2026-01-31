// test run
// target riscv32.q32

// ============================================================================
// Divide: mat2 / mat2 -> mat2 (component-wise)
// ============================================================================

mat2 test_mat2_divide_simple() {
    // Division with matrices (component-wise)
    mat2 a = mat2(6.0, 8.0, 10.0, 12.0);
    mat2 b = mat2(2.0, 4.0, 5.0, 6.0);
    return a / b;
}

// run: test_mat2_divide_simple() ~= mat2(3.0, 2.0, 2.0, 2.0) [expect-fail]

mat2 test_mat2_divide_identity() {
    mat2 a = mat2(2.0, 4.0, 6.0, 8.0);
    mat2 b = mat2(1.0, 1.0, 1.0, 1.0); // identity for division
    return a / b;
}

// run: test_mat2_divide_identity() ~= mat2(2.0, 4.0, 6.0, 8.0) [expect-fail]

mat2 test_mat2_divide_variables() {
    mat2 a = mat2(15.0, 20.0, 25.0, 30.0);
    mat2 b = mat2(3.0, 4.0, 5.0, 6.0);
    return a / b;
}

// run: test_mat2_divide_variables() ~= mat2(5.0, 5.0, 5.0, 5.0) [expect-fail]

mat2 test_mat2_divide_expressions() {
    return mat2(8.0, 12.0, 16.0, 20.0) / mat2(2.0, 3.0, 4.0, 5.0);
}

// run: test_mat2_divide_expressions() ~= mat2(4.0, 4.0, 4.0, 4.0) [expect-fail]

mat2 test_mat2_divide_in_assignment() {
    mat2 result = mat2(10.0, 15.0, 20.0, 25.0);
    result = result / mat2(2.0, 3.0, 4.0, 5.0);
    return result;
}

// run: test_mat2_divide_in_assignment() ~= mat2(5.0, 5.0, 5.0, 5.0) [expect-fail]

mat2 test_mat2_divide_scalar() {
    mat2 a = mat2(10.0, 20.0, 30.0, 40.0);
    return a / 2.0;
}

// run: test_mat2_divide_scalar() ~= mat2(5.0, 10.0, 15.0, 20.0) [expect-fail]

mat2 test_mat2_divide_scalar_variables() {
    mat2 a = mat2(12.0, 18.0, 24.0, 30.0);
    float s = 3.0;
    return a / s;
}

// run: test_mat2_divide_scalar_variables() ~= mat2(4.0, 6.0, 8.0, 10.0) [expect-fail]

mat2 test_mat2_divide_scalar_expressions() {
    return mat2(16.0, 24.0, 32.0, 40.0) / (4.0 * 2.0);
}

// run: test_mat2_divide_scalar_expressions() ~= mat2(2.0, 3.0, 4.0, 5.0) [expect-fail]

mat2 test_mat2_divide_scalar_assignment() {
    mat2 result = mat2(14.0, 21.0, 28.0, 35.0);
    result = result / 7.0;
    return result;
}

// run: test_mat2_divide_scalar_assignment() ~= mat2(2.0, 3.0, 4.0, 5.0) [expect-fail]




