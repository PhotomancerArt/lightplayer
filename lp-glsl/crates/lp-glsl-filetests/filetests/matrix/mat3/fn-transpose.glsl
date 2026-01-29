// test run
// target riscv32.q32

// ============================================================================
// Transpose: transpose(mat3) -> mat3
// ============================================================================

mat3 test_mat3_transpose_simple() {
    // Transpose swaps rows and columns
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    // m = [1, 2, 3]
    //     [4, 5, 6]
    //     [7, 8, 9]
    // transpose = [1, 4, 7]
    //            [2, 5, 8]
    //            [3, 6, 9]
    return transpose(m);
}

// run: test_mat3_transpose_simple() ~= mat3(1.0, 4.0, 7.0, 2.0, 5.0, 8.0, 3.0, 6.0, 9.0)

mat3 test_mat3_transpose_identity() {
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return transpose(m);
}

// run: test_mat3_transpose_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_transpose_symmetric() {
    mat3 m = mat3(1.0, 2.0, 3.0, 2.0, 4.0, 5.0, 3.0, 5.0, 6.0);
    return transpose(m);
}

// run: test_mat3_transpose_symmetric() ~= mat3(1.0, 2.0, 3.0, 2.0, 4.0, 5.0, 3.0, 5.0, 6.0)

mat3 test_mat3_transpose_asymmetric() {
    mat3 m = mat3(1.0, 4.0, 7.0, 2.0, 5.0, 8.0, 3.0, 6.0, 9.0);
    return transpose(m);
}

// run: test_mat3_transpose_asymmetric() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_transpose_double() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return transpose(transpose(m));
}

// run: test_mat3_transpose_double() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_transpose_variables() {
    mat3 m = mat3(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0);
    return transpose(m);
}

// run: test_mat3_transpose_variables() ~= mat3(5.0, 8.0, 11.0, 6.0, 9.0, 12.0, 7.0, 10.0, 13.0)

mat3 test_mat3_transpose_expressions() {
    return transpose(mat3(1.0, 3.0, 5.0, 2.0, 4.0, 6.0, 7.0, 8.0, 9.0));
}

// run: test_mat3_transpose_expressions() ~= mat3(1.0, 2.0, 7.0, 3.0, 4.0, 8.0, 5.0, 6.0, 9.0)

mat3 test_mat3_transpose_in_assignment() {
    mat3 result;
    result = transpose(mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0));
    return result;
}

// run: test_mat3_transpose_in_assignment() ~= mat3(1.0, 4.0, 7.0, 2.0, 5.0, 8.0, 3.0, 6.0, 9.0)

mat3 test_mat3_transpose_zero() {
    mat3 m = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return transpose(m);
}

// run: test_mat3_transpose_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_mat3_transpose_negative() {
    mat3 m = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    return transpose(m);
}

// run: test_mat3_transpose_negative() ~= mat3(-1.0, -4.0, -7.0, -2.0, -5.0, -8.0, -3.0, -6.0, -9.0)
