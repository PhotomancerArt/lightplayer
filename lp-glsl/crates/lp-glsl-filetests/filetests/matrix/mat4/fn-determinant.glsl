// test run
// target riscv32.q32

// ============================================================================
// Determinant: determinant(mat4) -> float
// ============================================================================

float test_mat4_determinant_identity() {
    // Determinant of identity matrix
    mat4 m = mat4(1.0);
    return determinant(m);
}

// run: test_mat4_determinant_identity() ~= 1.0

float test_mat4_determinant_simple() {
    // Simple 4x4 matrix determinant
    mat4 m = mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    return determinant(m);
}

// run: test_mat4_determinant_simple() ~= 1.0

float test_mat4_determinant_zero() {
    mat4 m = mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return determinant(m);
}

// run: test_mat4_determinant_zero() ~= 0.0

float test_mat4_determinant_diagonal() {
    mat4 m = mat4(2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 5.0);
    return determinant(m);
}

// run: test_mat4_determinant_diagonal() ~= 120.0

float test_mat4_determinant_variables() {
    mat4 m = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    return determinant(m);
}

// run: test_mat4_determinant_variables() ~= 0.0

float test_mat4_determinant_expressions() {
    return determinant(mat4(1.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0));
}

// run: test_mat4_determinant_expressions() ~= 24.0

float test_mat4_determinant_in_assignment() {
    float result;
    result = determinant(mat4(2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0));
    return result;
}

// run: test_mat4_determinant_in_assignment() ~= 16.0

float test_mat4_determinant_negative() {
    mat4 m = mat4(-1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0);
    return determinant(m);
}

// run: test_mat4_determinant_negative() ~= -1.0 [expect-fail]

float test_mat4_determinant_fractional() {
    mat4 m = mat4(0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5);
    return determinant(m);
}

// run: test_mat4_determinant_fractional() ~= 0.0625

float test_mat4_determinant_properties() {
    // Test that det(transpose(m)) == det(m)
    mat4 m = mat4(1.0, 2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    return determinant(transpose(m)) - determinant(m);
}

// run: test_mat4_determinant_properties() ~= 0.0




