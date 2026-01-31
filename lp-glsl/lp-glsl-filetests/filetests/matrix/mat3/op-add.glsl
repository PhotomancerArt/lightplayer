// test run
// target riscv32.q32

// ============================================================================
// Add: mat3 + mat3 -> mat3 (component-wise)
// ============================================================================

mat3 test_mat3_add_simple() {
    // Addition with 3x3 matrices (component-wise)
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5);
    return a + b;
}

// run: test_mat3_add_simple() ~= mat3(1.5, 3.5, 5.5, 7.5, 9.5, 11.5, 13.5, 15.5, 17.5)

mat3 test_mat3_add_identity() {
    mat3 a = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0); // identity matrix
    mat3 b = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0); // zero matrix
    return a + b;
}

// run: test_mat3_add_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_add_negative() {
    mat3 a = mat3(1.0, -2.0, 3.0, -4.0, 5.0, -6.0, 7.0, -8.0, 9.0);
    mat3 b = mat3(-1.0, 2.0, -3.0, 4.0, -5.0, 6.0, -7.0, 8.0, -9.0);
    return a + b;
}

// run: test_mat3_add_negative() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_mat3_add_variables() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(4.0, 3.0, 2.0, 1.0, 9.0, 8.0, 7.0, 6.0, 5.0);
    return a + b;
}

// run: test_mat3_add_variables() ~= mat3(5.0, 5.0, 5.0, 5.0, 14.0, 14.0, 14.0, 14.0, 14.0)

mat3 test_mat3_add_expressions() {
    return mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0) + mat3(0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5);
}

// run: test_mat3_add_expressions() ~= mat3(1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5)

mat3 test_mat3_add_in_assignment() {
    mat3 result = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result = result + mat3(0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9);
    return result;
}

// run: test_mat3_add_in_assignment() ~= mat3(1.1, 2.2, 3.3, 4.4, 5.5, 6.6, 7.7, 8.8, 9.9)

mat3 test_mat3_add_large_values() {
    mat3 a = mat3(1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0);
    mat3 b = mat3(1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0);
    return a + b;
}

// run: test_mat3_add_large_values() ~= mat3(2000.0, 4000.0, 6000.0, 8000.0, 10000.0, 12000.0, 14000.0, 16000.0, 18000.0)

mat3 test_mat3_add_zero_matrix() {
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 zero = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return a + zero;
}

// run: test_mat3_add_zero_matrix() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)
