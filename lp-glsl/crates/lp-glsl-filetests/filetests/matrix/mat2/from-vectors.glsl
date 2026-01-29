// test run
// target riscv32.q32

// ============================================================================
// From Vectors: mat2(vec2, vec2) - construct matrix from column vectors
// ============================================================================

mat2 test_mat2_from_vec2_columns() {
    // Constructor mat2(vec2, vec2) - each vec2 becomes a column
    vec2 col0 = vec2(1.0, 2.0);
    vec2 col1 = vec2(3.0, 4.0);
    return mat2(col0, col1);
}

// run: test_mat2_from_vec2_columns() ~= mat2(1.0, 2.0, 3.0, 4.0)

mat2 test_mat2_from_vec2_expressions() {
    return mat2(vec2(1.0, 2.0) + vec2(0.5, 0.5), vec2(3.0, 4.0) * vec2(1.0, 1.0));
}

// run: test_mat2_from_vec2_expressions() ~= mat2(1.5, 2.5, 3.0, 4.0)

mat2 test_mat2_from_vec2_variables() {
    vec2 a = vec2(5.0, 6.0);
    vec2 b = vec2(7.0, 8.0);
    return mat2(a, b);
}

// run: test_mat2_from_vec2_variables() ~= mat2(5.0, 6.0, 7.0, 8.0)

mat2 test_mat2_from_vec2_identity() {
    vec2 col0 = vec2(1.0, 0.0);
    vec2 col1 = vec2(0.0, 1.0);
    return mat2(col0, col1);
}

// run: test_mat2_from_vec2_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_mat2_from_vec2_zero() {
    vec2 zero = vec2(0.0, 0.0);
    return mat2(zero, zero);
}

// run: test_mat2_from_vec2_zero() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_from_vec2_negative() {
    return mat2(vec2(-1.0, -2.0), vec2(-3.0, -4.0));
}

// run: test_mat2_from_vec2_negative() ~= mat2(-1.0, -2.0, -3.0, -4.0)

mat2 test_mat2_from_vec2_mixed() {
    vec2 col0 = vec2(1.0, -2.0);
    vec2 col1 = vec2(3.5, 4.5);
    return mat2(col0, col1);
}

// run: test_mat2_from_vec2_mixed() ~= mat2(1.0, -2.0, 3.5, 4.5)

mat2 test_mat2_from_vec2_in_assignment() {
    mat2 result;
    result = mat2(vec2(2.0, 3.0), vec2(4.0, 5.0));
    return result;
}

// run: test_mat2_from_vec2_in_assignment() ~= mat2(2.0, 3.0, 4.0, 5.0)

mat2 test_mat2_from_vec2_function_result() {
    return mat2(normalize(vec2(3.0, 4.0)), vec2(1.0, 0.0));
}

// run: test_mat2_from_vec2_function_result() ~= mat2(0.6, 0.8, 1.0, 0.0)




