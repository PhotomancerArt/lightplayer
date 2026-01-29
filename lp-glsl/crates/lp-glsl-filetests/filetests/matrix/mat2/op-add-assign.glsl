// test run
// target riscv32.q32

// ============================================================================
// Add Assign: mat2 += mat2 (component-wise addition)
// ============================================================================

mat2 test_mat2_add_assign_simple() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result += mat2(0.5, 1.5, 2.5, 3.5);
    return result;
}

// run: test_mat2_add_assign_simple() ~= mat2(1.5, 3.5, 5.5, 7.5)

mat2 test_mat2_add_assign_identity() {
    mat2 result = mat2(1.0, 0.0, 0.0, 1.0);
    result += mat2(0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat2_add_assign_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_mat2_add_assign_variables() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(4.0, 3.0, 2.0, 1.0);
    a += b;
    return a;
}

// run: test_mat2_add_assign_variables() ~= mat2(5.0, 5.0, 5.0, 5.0)

mat2 test_mat2_add_assign_expressions() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result += mat2(1.0, 1.0, 1.0, 1.0) + mat2(0.5, 0.5, 0.5, 0.5);
    return result;
}

// run: test_mat2_add_assign_expressions() ~= mat2(2.5, 3.5, 4.5, 5.5)

mat2 test_mat2_add_assign_zero() {
    mat2 result = mat2(5.0, 6.0, 7.0, 8.0);
    result += mat2(0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat2_add_assign_zero() ~= mat2(5.0, 6.0, 7.0, 8.0)

mat2 test_mat2_add_assign_negative() {
    mat2 result = mat2(1.0, -2.0, 3.0, -4.0);
    result += mat2(-1.0, 2.0, -3.0, 4.0);
    return result;
}

// run: test_mat2_add_assign_negative() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_add_assign_chained() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(2.0, 3.0, 4.0, 5.0);
    a += b;
    b += a;
    return b;
}

// run: test_mat2_add_assign_chained() ~= mat2(5.0, 7.0, 9.0, 11.0) [expect-fail]

mat2 test_mat2_add_assign_large_values() {
    mat2 result = mat2(1000.0, 2000.0, 3000.0, 4000.0);
    result += mat2(1000.0, 2000.0, 3000.0, 4000.0);
    return result;
}

// run: test_mat2_add_assign_large_values() ~= mat2(2000.0, 4000.0, 6000.0, 8000.0)




