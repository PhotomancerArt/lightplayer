// test run
// target riscv32.q32

// ============================================================================
// Assign: mat2 = mat2 (assignment)
// ============================================================================

mat2 test_mat2_assign_simple() {
    mat2 result;
    result = mat2(1.0, 2.0, 3.0, 4.0);
    return result;
}

// run: test_mat2_assign_simple() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_assign_variables() {
    mat2 a = mat2(5.0, 6.0, 7.0, 8.0);
    mat2 result;
    result = a;
    return result;
}

// run: test_mat2_assign_variables() ~= mat2(5.0, 6.0, 7.0, 8.0)

mat2 test_mat2_assign_expressions() {
    mat2 result;
    result = mat2(1.0, 2.0, 3.0, 4.0) + mat2(0.5, 0.5, 0.5, 0.5);
    return result;
}

// run: test_mat2_assign_expressions() ~= mat2(1.5, 2.5, 3.5, 4.5)

mat2 test_mat2_assign_identity() {
    mat2 result;
    result = mat2(1.0, 0.0, 0.0, 1.0);
    return result;
}

// run: test_mat2_assign_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_mat2_assign_zero() {
    mat2 result;
    result = mat2(0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat2_assign_zero() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_assign_negative() {
    mat2 result;
    result = mat2(-1.0, -2.0, -3.0, -4.0);
    return result;
}

// run: test_mat2_assign_negative() ~= mat2(-1.0, -2.0, -3.0, -4.0)

mat2 test_mat2_assign_chained() {
    mat2 a, b;
    a = mat2(1.0, 2.0, 3.0, 4.0);
    b = a;
    return b;
}

// run: test_mat2_assign_chained() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_assign_function_result() {
    mat2 result;
    result = mat2(2.0); // Creates diagonal matrix
    return result;
}

// run: test_mat2_assign_function_result() ~= mat2(2.0, 0.0, 0.0, 2.0)

mat2 test_mat2_assign_after_operations() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(2.0, 3.0, 4.0, 5.0);
    a = a * b; // Matrix multiplication
    return a;
}

// run: test_mat2_assign_after_operations() ~= mat2(10.0, 13.0, 22.0, 29.0) [expect-fail]




