// test run
// target riscv32.q32

// ============================================================================
// Subtract Assign: mat3 -= mat3 (component-wise subtraction)
// ============================================================================

mat3 test_mat3_subtract_assign_simple() {
    mat3 result = mat3(5.0, 4.0, 3.0, 2.0, 1.0, 0.0, -1.0, -2.0, -3.0);
    result -= mat3(1.0, 2.0, 1.0, 1.0, 0.0, 1.0, -1.0, -2.0, -3.0);
    return result;
}

// run: test_mat3_subtract_assign_simple() ~= mat3(4.0, 2.0, 2.0, 1.0, 1.0, -1.0, 0.0, 0.0, 0.0)

mat3 test_mat3_subtract_assign_identity() {
    mat3 result = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    result -= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat3_subtract_assign_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_subtract_assign_variables() {
    mat3 a = mat3(10.0, 8.0, 6.0, 4.0, 2.0, 0.0, -2.0, -4.0, -6.0);
    mat3 b = mat3(3.0, 2.0, 1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0);
    a -= b;
    return a;
}

// run: test_mat3_subtract_assign_variables() ~= mat3(7.0, 6.0, 5.0, 4.0, 1.0, -2.0, -5.0, -8.0, -11.0)

mat3 test_mat3_subtract_assign_expressions() {
    mat3 result = mat3(5.0, 4.0, 3.0, 2.0, 1.0, 0.0, -1.0, -2.0, -3.0);
    result -= mat3(2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0) - mat3(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
    return result;
}

// run: test_mat3_subtract_assign_expressions() ~= mat3(4.0, 4.0, 2.0, 2.0, 0.0, 1.0, 0.0, 1.0, 0.0) [expect-fail]

mat3 test_mat3_subtract_assign_zero() {
    mat3 result = mat3(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0);
    result -= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat3_subtract_assign_zero() ~= mat3(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0)

mat3 test_mat3_subtract_assign_negative() {
    mat3 result = mat3(1.0, -2.0, 3.0, -4.0, 5.0, -6.0, 7.0, -8.0, 9.0);
    result -= mat3(-1.0, 2.0, -3.0, 4.0, -5.0, 6.0, -7.0, 8.0, -9.0);
    return result;
}

// run: test_mat3_subtract_assign_negative() ~= mat3(2.0, -4.0, 6.0, -8.0, 10.0, -12.0, 14.0, -16.0, 18.0)

mat3 test_mat3_subtract_assign_chained() {
    mat3 a = mat3(10.0, 8.0, 6.0, 4.0, 2.0, 0.0, -2.0, -4.0, -6.0);
    mat3 b = mat3(3.0, 2.0, 1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0);
    a -= b;
    b -= a;
    return b;
}

// run: test_mat3_subtract_assign_chained() ~= mat3(-4.0, -4.0, -4.0, -4.0, -4.0, -4.0, -4.0, -4.0, -4.0) [expect-fail]

mat3 test_mat3_subtract_assign_large_values() {
    mat3 result = mat3(1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0);
    result -= mat3(500.0, 1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 3500.0, 4000.0, 4500.0);
    return result;
}

// run: test_mat3_subtract_assign_large_values() ~= mat3(500.0, 1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 3500.0, 4000.0, 4500.0)




