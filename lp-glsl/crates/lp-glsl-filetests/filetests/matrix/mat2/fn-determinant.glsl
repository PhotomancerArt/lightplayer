// test run
// target riscv32.q32

// ============================================================================
// Determinant: determinant(mat2) -> float
// ============================================================================

float test_mat2_determinant_identity() {
    // Determinant of identity matrix
    mat2 m = mat2(1.0, 0.0, 0.0, 1.0);
    return determinant(m);
}

// run: test_mat2_determinant_identity() ~= 1.0

float test_mat2_determinant_simple() {
    // Determinant: for [a, b; c, d] = a*d - b*c
    mat2 m = mat2(2.0, 3.0, 4.0, 5.0);
    // det = 2*5 - 3*4 = 10 - 12 = -2
    return determinant(m);
}

// run: test_mat2_determinant_simple() ~= -2.0

float test_mat2_determinant_zero() {
    mat2 m = mat2(0.0, 0.0, 0.0, 0.0);
    return determinant(m);
}

// run: test_mat2_determinant_zero() ~= 0.0

float test_mat2_determinant_diagonal() {
    mat2 m = mat2(2.0, 0.0, 0.0, 3.0);
    return determinant(m);
}

// run: test_mat2_determinant_diagonal() ~= 6.0

float test_mat2_determinant_variables() {
    mat2 m = mat2(1.5, 2.5, 3.5, 4.5);
    return determinant(m);
}

// run: test_mat2_determinant_variables() ~= -2.0

float test_mat2_determinant_expressions() {
    return determinant(mat2(1.0, 2.0, 3.0, 4.0));
}

// run: test_mat2_determinant_expressions() ~= -2.0

float test_mat2_determinant_in_assignment() {
    float result;
    result = determinant(mat2(3.0, 1.0, 2.0, 4.0));
    return result;
}

// run: test_mat2_determinant_in_assignment() ~= 10.0

float test_mat2_determinant_negative() {
    mat2 m = mat2(-1.0, -2.0, -3.0, -4.0);
    return determinant(m);
}

// run: test_mat2_determinant_negative() ~= -2.0

float test_mat2_determinant_fractional() {
    mat2 m = mat2(0.5, 1.5, 2.5, 3.5);
    return determinant(m);
}

// run: test_mat2_determinant_fractional() ~= -2.0

float test_mat2_determinant_properties() {
    // Test that det(transpose(m)) == det(m)
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    return determinant(transpose(m)) - determinant(m);
}

// run: test_mat2_determinant_properties() ~= 0.0
