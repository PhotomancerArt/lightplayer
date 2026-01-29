// test run
// target riscv32.q32

// ============================================================================
// Subtract Assign: mat2 -= mat2 (component-wise subtraction)
// ============================================================================

mat2 test_mat2_subtract_assign_simple() {
    mat2 result = mat2(5.0, 4.0, 3.0, 2.0);
    result -= mat2(1.0, 2.0, 1.0, 1.0);
    return result;
}

// run: test_mat2_subtract_assign_simple() ~= mat2(4.0, 2.0, 2.0, 1.0)

mat2 test_mat2_subtract_assign_identity() {
    mat2 result = mat2(1.0, 0.0, 0.0, 1.0);
    result -= mat2(0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat2_subtract_assign_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_mat2_subtract_assign_variables() {
    mat2 a = mat2(10.0, 8.0, 6.0, 4.0);
    mat2 b = mat2(3.0, 2.0, 1.0, 0.0);
    a -= b;
    return a;
}

// run: test_mat2_subtract_assign_variables() ~= mat2(7.0, 6.0, 5.0, 4.0)

mat2 test_mat2_subtract_assign_expressions() {
    mat2 result = mat2(5.0, 4.0, 3.0, 2.0);
    result -= mat2(2.0, 1.0, 2.0, 1.0) - mat2(1.0, 1.0, 1.0, 1.0);
    return result;
}

// run: test_mat2_subtract_assign_expressions() ~= mat2(4.0, 4.0, 2.0, 2.0)

mat2 test_mat2_subtract_assign_zero() {
    mat2 result = mat2(5.0, 6.0, 7.0, 8.0);
    result -= mat2(0.0, 0.0, 0.0, 0.0);
    return result;
}

// run: test_mat2_subtract_assign_zero() ~= mat2(5.0, 6.0, 7.0, 8.0)

mat2 test_mat2_subtract_assign_negative() {
    mat2 result = mat2(1.0, -2.0, 3.0, -4.0);
    result -= mat2(-1.0, 2.0, -3.0, 4.0);
    return result;
}

// run: test_mat2_subtract_assign_negative() ~= mat2(2.0, -4.0, 6.0, -8.0)

mat2 test_mat2_subtract_assign_chained() {
    mat2 a = mat2(10.0, 8.0, 6.0, 4.0);
    mat2 b = mat2(3.0, 2.0, 1.0, 0.0);
    a -= b;
    b -= a;
    return b;
}

// run: test_mat2_subtract_assign_chained() ~= mat2(-4.0, -4.0, -4.0, -4.0)

mat2 test_mat2_subtract_assign_large_values() {
    mat2 result = mat2(1000.0, 2000.0, 3000.0, 4000.0);
    result -= mat2(500.0, 1000.0, 1500.0, 2000.0);
    return result;
}

// run: test_mat2_subtract_assign_large_values() ~= mat2(500.0, 1000.0, 1500.0, 2000.0)




