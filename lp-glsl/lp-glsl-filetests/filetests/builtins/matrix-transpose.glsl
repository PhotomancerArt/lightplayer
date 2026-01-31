// test run
// target riscv32.q32

// ============================================================================
// transpose(): Matrix transpose function
// transpose(mat) - returns transpose of matrix
// ============================================================================

mat2 test_transpose_mat2() {
    // transpose of 2x2 matrix
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    return transpose(m);
}

// run: test_transpose_mat2() ~= mat2(1.0, 3.0, 2.0, 4.0) [expect-fail]

mat3 test_transpose_mat3() {
    // transpose of 3x3 matrix
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return transpose(m);
}

// run: test_transpose_mat3() ~= mat3(1.0, 4.0, 7.0, 2.0, 5.0, 8.0, 3.0, 6.0, 9.0) [expect-fail]

mat4 test_transpose_mat4() {
    // transpose of 4x4 identity matrix
    mat4 m = mat4(1.0);
    return transpose(m);
}

// run: test_transpose_mat4() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0) [expect-fail]

mat2x3 test_transpose_mat2x3() {
    // transpose of 2x3 matrix (becomes 3x2)
    mat2x3 m = mat2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
    return transpose(m);
}

// run: test_transpose_mat2x3() ~= mat3x2(1.0, 4.0, 2.0, 5.0, 3.0, 6.0) [expect-fail]

mat3x2 test_transpose_mat3x2() {
    // transpose of 3x2 matrix (becomes 2x3)
    mat3x2 m = mat3x2(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
    return transpose(m);
}

// run: test_transpose_mat3x2() ~= mat2x3(1.0, 3.0, 5.0, 2.0, 4.0, 6.0) [expect-fail]

mat2 test_transpose_mat2_identity() {
    mat2 m = mat2(1.0, 0.0, 0.0, 1.0);
    return transpose(m);
}

// run: test_transpose_mat2_identity() ~= mat2(1.0, 0.0, 0.0, 1.0) [expect-fail]

mat3 test_transpose_mat3_identity() {
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return transpose(m);
}

// run: test_transpose_mat3_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0) [expect-fail]

mat4 test_transpose_mat4_simple() {
    mat4 m = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    return transpose(m);
}

// run: test_transpose_mat4_simple() ~= mat4(1.0, 5.0, 9.0, 13.0, 2.0, 6.0, 10.0, 14.0, 3.0, 7.0, 11.0, 15.0, 4.0, 8.0, 12.0, 16.0) [expect-fail]

mat2 test_transpose_mat2_negative() {
    mat2 m = mat2(-1.0, -2.0, -3.0, -4.0);
    return transpose(m);
}

// run: test_transpose_mat2_negative() ~= mat2(-1.0, -3.0, -2.0, -4.0) [expect-fail]

mat3 test_transpose_mat3_negative() {
    mat3 m = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    return transpose(m);
}

// run: test_transpose_mat3_negative() ~= mat3(-1.0, -4.0, -7.0, -2.0, -5.0, -8.0, -3.0, -6.0, -9.0) [expect-fail]

mat4 test_transpose_mat4_negative() {
    mat4 m = mat4(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, -13.0, -14.0, -15.0, -16.0);
    return transpose(m);
}

// run: test_transpose_mat4_negative() ~= mat4(-1.0, -5.0, -9.0, -13.0, -2.0, -6.0, -10.0, -14.0, -3.0, -7.0, -11.0, -15.0, -4.0, -8.0, -12.0, -16.0) [expect-fail]

mat2 test_transpose_mat2_fractions() {
    mat2 m = mat2(0.5, 1.5, 2.5, 3.5);
    return transpose(m);
}

// run: test_transpose_mat2_fractions() ~= mat2(0.5, 2.5, 1.5, 3.5) [expect-fail]

mat3 test_transpose_mat3_fractions() {
    mat3 m = mat3(0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5);
    return transpose(m);
}

// run: test_transpose_mat3_fractions() ~= mat3(0.5, 3.5, 6.5, 1.5, 4.5, 7.5, 2.5, 5.5, 8.5) [expect-fail]

mat4 test_transpose_mat4_fractions() {
    mat4 m = mat4(0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5, 10.5, 11.5, 12.5, 13.5, 14.5, 15.5);
    return transpose(m);
}

// run: test_transpose_mat4_fractions() ~= mat4(0.5, 4.5, 8.5, 12.5, 1.5, 5.5, 9.5, 13.5, 2.5, 6.5, 10.5, 14.5, 3.5, 7.5, 11.5, 15.5) [expect-fail]

mat2 test_transpose_mat2_expressions() {
    return transpose(mat2(1.0, 3.0, 2.0, 4.0));
}

// run: test_transpose_mat2_expressions() ~= mat2(1.0, 2.0, 3.0, 4.0) [expect-fail]

mat3 test_transpose_mat3_expressions() {
    return transpose(mat3(1.0, 4.0, 7.0, 2.0, 5.0, 8.0, 3.0, 6.0, 9.0));
}

// run: test_transpose_mat3_expressions() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0) [expect-fail]

mat4 test_transpose_mat4_expressions() {
    return transpose(mat4(1.0, 5.0, 9.0, 13.0, 2.0, 6.0, 10.0, 14.0, 3.0, 7.0, 11.0, 15.0, 4.0, 8.0, 12.0, 16.0));
}

// run: test_transpose_mat4_expressions() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0) [expect-fail]

mat2 test_transpose_mat2_variables() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    return transpose(m);
}

// run: test_transpose_mat2_variables() ~= mat2(1.0, 3.0, 2.0, 4.0) [expect-fail]

mat3 test_transpose_mat3_variables() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return transpose(m);
}

// run: test_transpose_mat3_variables() ~= mat3(1.0, 4.0, 7.0, 2.0, 5.0, 8.0, 3.0, 6.0, 9.0) [expect-fail]

mat4 test_transpose_mat4_variables() {
    mat4 m = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    return transpose(m);
}

// run: test_transpose_mat4_variables() ~= mat4(1.0, 5.0, 9.0, 13.0, 2.0, 6.0, 10.0, 14.0, 3.0, 7.0, 11.0, 15.0, 4.0, 8.0, 12.0, 16.0) [expect-fail]
