// test run
// target riscv32.q32

// ============================================================================
// Subtract: mat2 - mat2 -> mat2 (component-wise)
// ============================================================================

mat2 test_mat2_subtract_simple() {
    // Subtraction with matrices (component-wise)
    mat2 a = mat2(5.0, 4.0, 3.0, 2.0);
    mat2 b = mat2(1.0, 2.0, 1.0, 1.0);
    return a - b;
}

// run: test_mat2_subtract_simple() ~= mat2(4.0, 2.0, 2.0, 1.0)

mat2 test_mat2_subtract_identity() {
    mat2 a = mat2(1.0, 0.0, 0.0, 1.0); // identity matrix
    mat2 b = mat2(0.0, 0.0, 0.0, 0.0); // zero matrix
    return a - b;
}

// run: test_mat2_subtract_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_mat2_subtract_negative() {
    mat2 a = mat2(1.0, -2.0, 3.0, -4.0);
    mat2 b = mat2(-1.0, 2.0, -3.0, 4.0);
    return a - b;
}

// run: test_mat2_subtract_negative() ~= mat2(2.0, -4.0, 6.0, -8.0)

mat2 test_mat2_subtract_variables() {
    mat2 a = mat2(10.0, 8.0, 6.0, 4.0);
    mat2 b = mat2(3.0, 2.0, 1.0, 0.0);
    return a - b;
}

// run: test_mat2_subtract_variables() ~= mat2(7.0, 6.0, 5.0, 4.0)

mat2 test_mat2_subtract_expressions() {
    return mat2(5.0, 4.0, 3.0, 2.0) - mat2(2.0, 1.0, 2.0, 1.0);
}

// run: test_mat2_subtract_expressions() ~= mat2(3.0, 3.0, 1.0, 1.0)

mat2 test_mat2_subtract_in_assignment() {
    mat2 result = mat2(10.0, 8.0, 6.0, 4.0);
    result = result - mat2(1.0, 2.0, 3.0, 4.0);
    return result;
}

// run: test_mat2_subtract_in_assignment() ~= mat2(9.0, 6.0, 3.0, 0.0)

mat2 test_mat2_subtract_large_values() {
    mat2 a = mat2(1000.0, 2000.0, 3000.0, 4000.0);
    mat2 b = mat2(500.0, 1000.0, 1500.0, 2000.0);
    return a - b;
}

// run: test_mat2_subtract_large_values() ~= mat2(500.0, 1000.0, 1500.0, 2000.0)

mat2 test_mat2_subtract_zero_matrix() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 zero = mat2(0.0, 0.0, 0.0, 0.0);
    return a - zero;
}

// run: test_mat2_subtract_zero_matrix() ~= mat2(1.0, 2.0, 3.0, 4.0)




