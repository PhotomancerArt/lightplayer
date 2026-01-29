// test run
// target riscv32.q32

// ============================================================================
// Subtract: mat3 - mat3 -> mat3 (component-wise)
// ============================================================================

mat3 test_mat3_subtract_simple() {
    // Subtraction with matrices (component-wise)
    mat3 a = mat3(5.0, 4.0, 3.0, 2.0, 1.0, 0.0, -1.0, -2.0, -3.0);
    mat3 b = mat3(1.0, 2.0, 1.0, 1.0, 0.0, 1.0, -1.0, -2.0, -3.0);
    return a - b;
}

// run: test_mat3_subtract_simple() ~= mat3(4.0, 2.0, 2.0, 1.0, 1.0, -1.0, 0.0, 0.0, 0.0)

mat3 test_mat3_subtract_identity() {
    mat3 a = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0); // identity matrix
    mat3 b = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0); // zero matrix
    return a - b;
}

// run: test_mat3_subtract_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_subtract_negative() {
    mat3 a = mat3(1.0, -2.0, 3.0, -4.0, 5.0, -6.0, 7.0, -8.0, 9.0);
    mat3 b = mat3(-1.0, 2.0, -3.0, 4.0, -5.0, 6.0, -7.0, 8.0, -9.0);
    return a - b;
}

// run: test_mat3_subtract_negative() ~= mat3(2.0, -4.0, 6.0, -8.0, 10.0, -12.0, 14.0, -16.0, 18.0)

mat3 test_mat3_subtract_variables() {
    mat3 a = mat3(10.0, 8.0, 6.0, 4.0, 2.0, 0.0, -2.0, -4.0, -6.0);
    mat3 b = mat3(3.0, 2.0, 1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0);
    return a - b;
}

// run: test_mat3_subtract_variables() ~= mat3(7.0, 6.0, 5.0, 4.0, 1.0, -2.0, -5.0, -8.0, -11.0)

mat3 test_mat3_subtract_expressions() {
    return mat3(5.0, 4.0, 3.0, 2.0, 1.0, 0.0, -1.0, -2.0, -3.0) - mat3(2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0);
}

// run: test_mat3_subtract_expressions() ~= mat3(3.0, 3.0, 1.0, 1.0, -1.0, -1.0, -3.0, -3.0, -5.0)

mat3 test_mat3_subtract_in_assignment() {
    mat3 result = mat3(10.0, 8.0, 6.0, 4.0, 2.0, 0.0, -2.0, -4.0, -6.0);
    result = result - mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return result;
}

// run: test_mat3_subtract_in_assignment() ~= mat3(9.0, 6.0, 3.0, 0.0, -3.0, -6.0, -9.0, -12.0, -15.0)

mat3 test_mat3_subtract_large_values() {
    mat3 a = mat3(1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0);
    mat3 b = mat3(500.0, 1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 3500.0, 4000.0, 4500.0);
    return a - b;
}

// run: test_mat3_subtract_large_values() ~= mat3(500.0, 1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 3500.0, 4000.0, 4500.0)

mat3 test_mat3_subtract_zero_matrix() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 zero = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return a - zero;
}

// run: test_mat3_subtract_zero_matrix() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)




