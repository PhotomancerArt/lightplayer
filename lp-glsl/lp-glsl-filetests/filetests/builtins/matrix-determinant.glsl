// test run
// target riscv32.q32

// ============================================================================
// determinant(): Matrix determinant function
// determinant(mat) - returns determinant of matrix
// ============================================================================

float test_determinant_mat2_identity() {
    // determinant of 2x2 identity matrix
    mat2 m = mat2(1.0, 0.0, 0.0, 1.0);
    return determinant(m);
}

// run: test_determinant_mat2_identity() ~= 1.0

float test_determinant_mat2_simple() {
    // determinant of 2x2 matrix: det([1,2; 3,4]) = 1*4 - 2*3 = -2
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    return determinant(m);
}

// run: test_determinant_mat2_simple() ~= -2.0

float test_determinant_mat2_zeros() {
    // determinant of singular 2x2 matrix
    mat2 m = mat2(1.0, 2.0, 2.0, 4.0);
    return determinant(m);
}

// run: test_determinant_mat2_zeros() ~= 0.0

float test_determinant_mat3_identity() {
    // determinant of 3x3 identity matrix
    mat3 m = mat3(1.0);
    return determinant(m);
}

// run: test_determinant_mat3_identity() ~= 1.0

float test_determinant_mat3_simple() {
    // determinant of 3x3 matrix
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return determinant(m);
}

// run: test_determinant_mat3_simple() ~= 0.0

float test_determinant_mat4_identity() {
    // determinant of 4x4 identity matrix
    mat4 m = mat4(1.0);
    return determinant(m);
}

// run: test_determinant_mat4_identity() ~= 1.0

float test_determinant_mat2_negative() {
    mat2 m = mat2(-1.0, -2.0, -3.0, -4.0);
    return determinant(m);
}

// run: test_determinant_mat2_negative() ~= -2.0

float test_determinant_mat3_negative() {
    mat3 m = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    return determinant(m);
}

// run: test_determinant_mat3_negative() ~= 0.0

float test_determinant_mat4_negative() {
    mat4 m = mat4(-1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0);
    return determinant(m);
}

// run: test_determinant_mat4_negative() ~= -1.0 [expect-fail]

float test_determinant_mat2_fractions() {
    mat2 m = mat2(0.5, 1.5, 2.5, 3.5);
    return determinant(m);
}

// run: test_determinant_mat2_fractions() ~= -2.0

float test_determinant_mat3_fractions() {
    mat3 m = mat3(0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5);
    return determinant(m);
}

// run: test_determinant_mat3_fractions() ~= 0.0

float test_determinant_mat4_fractions() {
    mat4 m = mat4(0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5);
    return determinant(m);
}

// run: test_determinant_mat4_fractions() ~= 0.0625

float test_determinant_mat2_diagonal() {
    mat2 m = mat2(2.0, 0.0, 0.0, 3.0);
    return determinant(m);
}

// run: test_determinant_mat2_diagonal() ~= 6.0

float test_determinant_mat3_diagonal() {
    mat3 m = mat3(2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0);
    return determinant(m);
}

// run: test_determinant_mat3_diagonal() ~= 24.0

float test_determinant_mat4_diagonal() {
    mat4 m = mat4(2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 5.0);
    return determinant(m);
}

// run: test_determinant_mat4_diagonal() ~= 120.0

float test_determinant_mat2_expressions() {
    return determinant(mat2(2.0, 1.0, 1.0, 1.0));
}

// run: test_determinant_mat2_expressions() ~= 1.0

float test_determinant_mat3_expressions() {
    return determinant(mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0));
}

// run: test_determinant_mat3_expressions() ~= 1.0

float test_determinant_mat4_expressions() {
    return determinant(mat4(2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0));
}

// run: test_determinant_mat4_expressions() ~= 16.0

float test_determinant_mat2_variables() {
    mat2 m = mat2(3.0, 1.0, 2.0, 4.0);
    return determinant(m);
}

// run: test_determinant_mat2_variables() ~= 10.0

float test_determinant_mat3_variables() {
    mat3 m = mat3(1.0, 2.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return determinant(m);
}

// run: test_determinant_mat3_variables() ~= 1.0

float test_determinant_mat4_variables() {
    mat4 m = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    return determinant(m);
}

// run: test_determinant_mat4_variables() ~= 0.0
