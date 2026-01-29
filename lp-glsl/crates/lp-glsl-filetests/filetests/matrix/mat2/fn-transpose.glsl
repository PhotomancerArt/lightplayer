// test run
// target riscv32.q32

// ============================================================================
// Transpose: transpose(mat2) -> mat2
// ============================================================================

mat2 test_mat2_transpose_simple() {
    // Transpose swaps rows and columns
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    // m = [1, 2]
    //     [3, 4]
    // transpose = [1, 3]
    //            [2, 4]
    return transpose(m);
}

// run: test_mat2_transpose_simple() ~= mat2(1.0, 3.0, 2.0, 4.0)

mat2 test_mat2_transpose_identity() {
    mat2 m = mat2(1.0, 0.0, 0.0, 1.0);
    return transpose(m);
}

// run: test_mat2_transpose_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_mat2_transpose_symmetric() {
    mat2 m = mat2(1.0, 2.0, 2.0, 3.0);
    return transpose(m);
}

// run: test_mat2_transpose_symmetric() ~= mat2(1.0, 2.0, 2.0, 3.0)

mat2 test_mat2_transpose_asymmetric() {
    mat2 m = mat2(1.0, 4.0, 2.0, 3.0);
    return transpose(m);
}

// run: test_mat2_transpose_asymmetric() ~= mat2(1.0, 2.0, 4.0, 3.0)

mat2 test_mat2_transpose_double() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    return transpose(transpose(m));
}

// run: test_mat2_transpose_double() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_transpose_variables() {
    mat2 m = mat2(5.0, 6.0, 7.0, 8.0);
    return transpose(m);
}

// run: test_mat2_transpose_variables() ~= mat2(5.0, 7.0, 6.0, 8.0)

mat2 test_mat2_transpose_expressions() {
    return transpose(mat2(1.0, 3.0, 2.0, 4.0));
}

// run: test_mat2_transpose_expressions() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_transpose_in_assignment() {
    mat2 result;
    result = transpose(mat2(1.0, 2.0, 3.0, 4.0));
    return result;
}

// run: test_mat2_transpose_in_assignment() ~= mat2(1.0, 3.0, 2.0, 4.0)

mat2 test_mat2_transpose_zero() {
    mat2 m = mat2(0.0, 0.0, 0.0, 0.0);
    return transpose(m);
}

// run: test_mat2_transpose_zero() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_transpose_negative() {
    mat2 m = mat2(-1.0, -2.0, -3.0, -4.0);
    return transpose(m);
}

// run: test_mat2_transpose_negative() ~= mat2(-1.0, -3.0, -2.0, -4.0)
