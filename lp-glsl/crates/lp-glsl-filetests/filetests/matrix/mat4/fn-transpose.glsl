// test run
// target riscv32.q32

// ============================================================================
// Transpose: transpose(mat4) -> mat4
// ============================================================================

mat4 test_mat4_transpose_simple() {
    // Transpose swaps rows and columns
    mat4 m = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    return transpose(m);
}

// run: test_mat4_transpose_simple() ~= mat4(1.0, 5.0, 9.0, 13.0, 2.0, 6.0, 10.0, 14.0, 3.0, 7.0, 11.0, 15.0, 4.0, 8.0, 12.0, 16.0)

mat4 test_mat4_transpose_identity() {
    mat4 m = mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    return transpose(m);
}

// run: test_mat4_transpose_identity() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_transpose_symmetric() {
    mat4 m = mat4(1.0, 2.0, 3.0, 4.0, 2.0, 5.0, 6.0, 7.0, 3.0, 6.0, 8.0, 9.0, 4.0, 7.0, 9.0, 10.0);
    return transpose(m);
}

// run: test_mat4_transpose_symmetric() ~= mat4(1.0, 2.0, 3.0, 4.0, 2.0, 5.0, 6.0, 7.0, 3.0, 6.0, 8.0, 9.0, 4.0, 7.0, 9.0, 10.0)

mat4 test_mat4_transpose_asymmetric() {
    mat4 m = mat4(1.0, 5.0, 9.0, 13.0, 2.0, 6.0, 10.0, 14.0, 3.0, 7.0, 11.0, 15.0, 4.0, 8.0, 12.0, 16.0);
    return transpose(m);
}

// run: test_mat4_transpose_asymmetric() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_transpose_double() {
    mat4 m = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    return transpose(transpose(m));
}

// run: test_mat4_transpose_double() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_transpose_variables() {
    mat4 m = mat4(16.0, 15.0, 14.0, 13.0, 12.0, 11.0, 10.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0);
    return transpose(m);
}

// run: test_mat4_transpose_variables() ~= mat4(16.0, 12.0, 8.0, 4.0, 15.0, 11.0, 7.0, 3.0, 14.0, 10.0, 6.0, 2.0, 13.0, 9.0, 5.0, 1.0)

mat4 test_mat4_transpose_expressions() {
    return transpose(mat4(1.0, 5.0, 9.0, 13.0, 2.0, 6.0, 10.0, 14.0, 3.0, 7.0, 11.0, 15.0, 4.0, 8.0, 12.0, 16.0));
}

// run: test_mat4_transpose_expressions() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_transpose_in_assignment() {
    mat4 result;
    result = transpose(mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0));
    return result;
}

// run: test_mat4_transpose_in_assignment() ~= mat4(1.0, 5.0, 9.0, 13.0, 2.0, 6.0, 10.0, 14.0, 3.0, 7.0, 11.0, 15.0, 4.0, 8.0, 12.0, 16.0)

mat4 test_mat4_transpose_zero() {
    mat4 m = mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return transpose(m);
}

// run: test_mat4_transpose_zero() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat4 test_mat4_transpose_negative() {
    mat4 m = mat4(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, -13.0, -14.0, -15.0, -16.0);
    return transpose(m);
}

// run: test_mat4_transpose_negative() ~= mat4(-1.0, -5.0, -9.0, -13.0, -2.0, -6.0, -10.0, -14.0, -3.0, -7.0, -11.0, -15.0, -4.0, -8.0, -12.0, -16.0)




