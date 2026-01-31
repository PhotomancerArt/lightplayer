// test run
// target riscv32.q32

// ============================================================================
// Add Assign: mat3 += mat3 (component-wise addition)
// ============================================================================

mat3 test_mat3_add_assign_simple() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result += mat3(0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5);
    return result;
}

// run: test_mat3_add_assign_simple() ~= mat3(1.5, 3.5, 5.5, 7.5, 9.5, 11.5, 13.5, 15.5, 17.5)

mat3 test_mat3_add_assign_identity() {
    mat3 result = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    result += mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat3_add_assign_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_add_assign_variables() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0);
    a += b;
    return a;
}

// run: test_mat3_add_assign_variables() ~= mat3(10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0)

mat3 test_mat3_add_assign_expressions() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result += mat3(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0) + mat3(0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5);
    return result;
}

// run: test_mat3_add_assign_expressions() ~= mat3(2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5, 10.5)

mat3 test_mat3_add_assign_zero() {
    mat3 result = mat3(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0);
    result += mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat3_add_assign_zero() ~= mat3(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0)

mat3 test_mat3_add_assign_negative() {
    mat3 result = mat3(1.0, -2.0, 3.0, -4.0, 5.0, -6.0, 7.0, -8.0, 9.0);
    result += mat3(-1.0, 2.0, -3.0, 4.0, -5.0, 6.0, -7.0, 8.0, -9.0);
    return result;
}

// run: test_mat3_add_assign_negative() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_mat3_add_assign_chained() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0);
    a += b;
    b += a;
    return b;
}

// run: test_mat3_add_assign_chained() ~= mat3(5.0, 7.0, 9.0, 11.0, 13.0, 15.0, 17.0, 19.0, 21.0) [expect-fail]

mat3 test_mat3_add_assign_large_values() {
    mat3 result = mat3(1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0);
    result += mat3(1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0);
    return result;
}

// run: test_mat3_add_assign_large_values() ~= mat3(2000.0, 4000.0, 6000.0, 8000.0, 10000.0, 12000.0, 14000.0, 16000.0, 18000.0)




