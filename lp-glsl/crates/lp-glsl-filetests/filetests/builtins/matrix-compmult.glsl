// test run
// target riscv32.q32

// ============================================================================
// matrixCompMult(): Matrix component-wise multiply function
// matrixCompMult(mat, mat) - component-wise matrix multiply
// ============================================================================

mat2 test_matrixcompmult_mat2_identity() {
    // matrixCompMult with identity matrices
    mat2 a = mat2(1.0, 0.0, 0.0, 1.0);
    mat2 b = mat2(1.0, 0.0, 0.0, 1.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat2_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_matrixcompmult_mat2_simple() {
    // matrixCompMult with simple 2x2 matrices
    mat2 a = mat2(2.0, 3.0, 4.0, 5.0);
    mat2 b = mat2(1.0, 2.0, 3.0, 4.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat2_simple() ~= mat2(2.0, 6.0, 12.0, 20.0)

mat3 test_matrixcompmult_mat3() {
    // matrixCompMult with 3x3 matrices
    mat3 a = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 b = mat3(2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat3() ~= mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0)

mat4 test_matrixcompmult_mat4() {
    // matrixCompMult with 4x4 matrices
    mat4 a = mat4(1.0);
    mat4 b = mat4(3.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat4() ~= mat4(3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0) [expect-fail]

mat2 test_matrixcompmult_mat2_zeros() {
    // matrixCompMult resulting in zeros
    mat2 a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 b = mat2(0.0, 0.0, 0.0, 0.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat2_zeros() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_matrixcompmult_mat2_negative() {
    // matrixCompMult with negative values
    mat2 a = mat2(-1.0, 2.0, -3.0, 4.0);
    mat2 b = mat2(2.0, -3.0, 4.0, -5.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat2_negative() ~= mat2(-2.0, -6.0, -12.0, -20.0)

mat3 test_matrixcompmult_mat3_negative() {
    mat3 a = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    mat3 b = mat3(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat3_negative() ~= mat3(-2.0, -6.0, -12.0, -20.0, -30.0, -42.0, -56.0, -72.0, -90.0)

mat4 test_matrixcompmult_mat4_negative() {
    mat4 a = mat4(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, -13.0, -14.0, -15.0, -16.0);
    mat4 b = mat4(2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat4_negative() ~= mat4(-2.0, -4.0, -6.0, -8.0, -10.0, -12.0, -14.0, -16.0, -18.0, -20.0, -22.0, -24.0, -26.0, -28.0, -30.0, -32.0)

mat2 test_matrixcompmult_mat2_fractions() {
    mat2 a = mat2(0.5, 1.5, 2.5, 3.5);
    mat2 b = mat2(2.0, 2.0, 2.0, 2.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat2_fractions() ~= mat2(1.0, 3.0, 5.0, 7.0)

mat3 test_matrixcompmult_mat3_fractions() {
    mat3 a = mat3(0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5);
    mat3 b = mat3(2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat3_fractions() ~= mat3(1.0, 3.0, 5.0, 7.0, 9.0, 11.0, 13.0, 15.0, 17.0)

mat4 test_matrixcompmult_mat4_fractions() {
    mat4 a = mat4(0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5, 10.5, 11.5, 12.5, 13.5, 14.5, 15.5);
    mat4 b = mat4(2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat4_fractions() ~= mat4(1.0, 3.0, 5.0, 7.0, 9.0, 11.0, 13.0, 15.0, 17.0, 19.0, 21.0, 23.0, 25.0, 27.0, 29.0, 31.0)

mat2 test_matrixcompmult_mat2_variables() {
    mat2 a = mat2(2.0, 4.0, 6.0, 8.0);
    mat2 b = mat2(3.0, 5.0, 7.0, 9.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat2_variables() ~= mat2(6.0, 20.0, 42.0, 72.0)

mat3 test_matrixcompmult_mat3_variables() {
    mat3 a = mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0);
    mat3 b = mat3(3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat3_variables() ~= mat3(6.0, 12.0, 18.0, 24.0, 30.0, 36.0, 42.0, 48.0, 54.0)

mat4 test_matrixcompmult_mat4_variables() {
    mat4 a = mat4(2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0);
    mat4 b = mat4(2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0);
    return matrixCompMult(a, b);
}

// run: test_matrixcompmult_mat4_variables() ~= mat4(4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 30.0, 32.0, 34.0)

mat2 test_matrixcompmult_mat2_expressions() {
    return matrixCompMult(mat2(2.0, 3.0, 4.0, 5.0), mat2(3.0, 4.0, 5.0, 6.0));
}

// run: test_matrixcompmult_mat2_expressions() ~= mat2(6.0, 12.0, 20.0, 30.0)

mat3 test_matrixcompmult_mat3_expressions() {
    return matrixCompMult(mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0), mat3(2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0));
}

// run: test_matrixcompmult_mat3_expressions() ~= mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0)

mat4 test_matrixcompmult_mat4_expressions() {
    return matrixCompMult(mat4(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0), mat4(2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0));
}

// run: test_matrixcompmult_mat4_expressions() ~= mat4(2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0)
