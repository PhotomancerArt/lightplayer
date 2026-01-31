// test run
// target riscv32.q32

// ============================================================================
// Assign: mat3 = mat3 (assignment)
// ============================================================================

mat3 test_mat3_assign_simple() {
    mat3 result;
    result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return result;
}

// run: test_mat3_assign_simple() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_assign_variables() {
    mat3 a = mat3(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0);
    mat3 result;
    result = a;
    return result;
}

// run: test_mat3_assign_variables() ~= mat3(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0)

mat3 test_mat3_assign_expressions() {
    mat3 result;
    result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0) + mat3(0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5);
    return result;
}

// run: test_mat3_assign_expressions() ~= mat3(1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5)

mat3 test_mat3_assign_identity() {
    mat3 result;
    result = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return result;
}

// run: test_mat3_assign_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_assign_zero() {
    mat3 result;
    result = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat3_assign_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_mat3_assign_negative() {
    mat3 result;
    result = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    return result;
}

// run: test_mat3_assign_negative() ~= mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0)

mat3 test_mat3_assign_chained() {
    mat3 a, b;
    a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    b = a;
    return b;
}

// run: test_mat3_assign_chained() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_assign_function_result() {
    mat3 result;
    result = mat3(2.0); // Creates diagonal matrix
    return result;
}

// run: test_mat3_assign_function_result() ~= mat3(2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0)

mat3 test_mat3_assign_after_operations() {
    mat3 a = mat3(1.0, 2.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    mat3 b = mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0);
    a = a * b; // Matrix multiplication
    return a;
}

// run: test_mat3_assign_after_operations() ~= mat3(2.0, 3.0, 4.0, 10.0, 11.0, 12.0, 8.0, 9.0, 10.0) [expect-fail]




