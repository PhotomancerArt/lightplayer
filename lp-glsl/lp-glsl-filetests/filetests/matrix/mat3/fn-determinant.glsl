// test run
// target riscv32.q32

// ============================================================================
// Determinant: determinant(mat3) -> float
// ============================================================================

float test_mat3_determinant_identity() {
    // Determinant of identity matrix
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return determinant(m);
}

// run: test_mat3_determinant_identity() ~= 1.0

float test_mat3_determinant_simple() {
    // Determinant of a simple 3x3 matrix
    // For [1, 2, 3; 4, 5, 6; 7, 8, 9]
    // det = 1*(5*9-6*8) - 2*(4*9-6*7) + 3*(4*8-5*7)
    //     = 1*(45-48) - 2*(36-42) + 3*(32-35)
    //     = 1*(-3) - 2*(-6) + 3*(-3)
    //     = -3 + 12 - 9 = 0
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return determinant(m);
}

// run: test_mat3_determinant_simple() ~= 0.0

float test_mat3_determinant_upper_triangular() {
    // Upper triangular matrix - determinant is product of diagonal
    mat3 m = mat3(2.0, 3.0, 4.0, 0.0, 5.0, 6.0, 0.0, 0.0, 7.0);
    return determinant(m);
}

// run: test_mat3_determinant_upper_triangular() ~= 70.0

float test_mat3_determinant_scale() {
    // Scale matrix - determinant should be product of scale factors
    mat3 m = mat3(2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0);
    return determinant(m);
}

// run: test_mat3_determinant_scale() ~= 24.0

float test_mat3_determinant_variables() {
    mat3 m = mat3(1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5);
    return determinant(m);
    // Complex calculation - result depends on actual determinant formula
}

// run: test_mat3_determinant_variables() ~= 0.0

float test_mat3_determinant_expressions() {
    return determinant(mat3(1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0));
}

// run: test_mat3_determinant_expressions() ~= 6.0

float test_mat3_determinant_in_assignment() {
    float result;
    result = determinant(mat3(1.0, 2.0, 3.0, 0.0, 1.0, 4.0, 5.0, 6.0, 0.0));
    return result;
    // Determinant calculation for this matrix
}

// run: test_mat3_determinant_in_assignment() ~= 1.0

float test_mat3_determinant_zero() {
    mat3 m = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return determinant(m);
}

// run: test_mat3_determinant_zero() ~= 0.0

float test_mat3_determinant_negative() {
    mat3 m = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    return determinant(m);
    // Determinant of negative matrix
}

// run: test_mat3_determinant_negative() ~= 0.0

float test_mat3_determinant_properties() {
    // Test that det(transpose(m)) == det(m)
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return determinant(transpose(m)) - determinant(m);
}

// run: test_mat3_determinant_properties() ~= 0.0
