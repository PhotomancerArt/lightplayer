// test run
// target riscv32.q32

// ============================================================================
// Add: mat2 + mat2 -> mat2 (component-wise)
// ============================================================================

mat2 test_mat2_add_simple() {
    // Addition with matrices (component-wise)
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(0.5, 1.5, 2.5, 3.5);
    return a + b;
}

// run: test_mat2_add_simple() ~= mat2(1.5, 3.5, 5.5, 7.5)

mat2 test_mat2_add_identity() {
    mat2 a = mat2(1.0, 0.0, 0.0, 1.0); // identity matrix
    mat2 b = mat2(0.0, 0.0, 0.0, 0.0); // zero matrix
    return a + b;
}

// run: test_mat2_add_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_mat2_add_negative() {
    mat2 a = mat2(1.0, -2.0, 3.0, -4.0);
    mat2 b = mat2(-1.0, 2.0, -3.0, 4.0);
    return a + b;
}

// run: test_mat2_add_negative() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_add_variables() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(4.0, 3.0, 2.0, 1.0);
    return a + b;
}

// run: test_mat2_add_variables() ~= mat2(5.0, 5.0, 5.0, 5.0)

mat2 test_mat2_add_expressions() {
    return mat2(1.0, 2.0, 3.0, 4.0) + mat2(0.5, 0.5, 0.5, 0.5);
}

// run: test_mat2_add_expressions() ~= mat2(1.5, 2.5, 3.5, 4.5)

mat2 test_mat2_add_in_assignment() {
    mat2 result = mat2(1.0, 2.0, 3.0, 4.0);
    result = result + mat2(0.1, 0.2, 0.3, 0.4);
    return result;
}

// run: test_mat2_add_in_assignment() ~= mat2(1.1, 2.2, 3.3, 4.4)

mat2 test_mat2_add_large_values() {
    mat2 a = mat2(1000.0, 2000.0, 3000.0, 4000.0);
    mat2 b = mat2(1000.0, 2000.0, 3000.0, 4000.0);
    return a + b;
}

// run: test_mat2_add_large_values() ~= mat2(2000.0, 4000.0, 6000.0, 8000.0)

mat2 test_mat2_add_zero_matrix() {
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 zero = mat2(0.0, 0.0, 0.0, 0.0);
    return a + zero;
}

// run: test_mat2_add_zero_matrix() ~= mat2(1.0, 2.0, 3.0, 4.0)
