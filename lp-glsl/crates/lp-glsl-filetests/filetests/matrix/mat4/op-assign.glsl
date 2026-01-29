// test run
// target riscv32.q32

// ============================================================================
// Assign: mat4 = mat4 (assignment)
// ============================================================================

mat4 test_mat4_assign_simple() {
    mat4 result;
    result = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    return result;
}

// run: test_mat4_assign_simple() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_assign_variables() {
    mat4 a = mat4(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0, 20.0);
    mat4 result;
    result = a;
    return result;
}

// run: test_mat4_assign_variables() ~= mat4(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0, 20.0)

mat4 test_mat4_assign_expressions() {
    mat4 result;
    result = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0) + mat4(0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5);
    return result;
}

// run: test_mat4_assign_expressions() ~= mat4(1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5, 10.5, 11.5, 12.5, 13.5, 14.5, 15.5, 16.5)

mat4 test_mat4_assign_identity() {
    mat4 result;
    result = mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    return result;
}

// run: test_mat4_assign_identity() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_assign_zero() {
    mat4 result;
    result = mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat4_assign_zero() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat4 test_mat4_assign_negative() {
    mat4 result;
    result = mat4(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, -13.0, -14.0, -15.0, -16.0);
    return result;
}

// run: test_mat4_assign_negative() ~= mat4(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, -13.0, -14.0, -15.0, -16.0)

mat4 test_mat4_assign_chained() {
    mat4 a, b;
    a = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    b = a;
    return b;
}

// run: test_mat4_assign_chained() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_assign_function_result() {
    mat4 result;
    result = mat4(2.0); // Creates diagonal matrix
    return result;
}

// run: test_mat4_assign_function_result() ~= mat4(2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0)

mat4 test_mat4_assign_after_operations() {
    mat4 a = mat4(1.0, 2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    mat4 b = mat4(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0);
    a = a * b; // Matrix multiplication
    return a;
}

// run: test_mat4_assign_after_operations() ~= mat4(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 18.0, 19.0, 20.0, 21.0, 14.0, 15.0, 16.0, 17.0) [expect-fail]




