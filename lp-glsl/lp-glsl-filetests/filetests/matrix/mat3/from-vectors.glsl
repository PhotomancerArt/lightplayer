// test run
// target riscv32.q32

// ============================================================================
// From Vectors: mat3(vec3, vec3, vec3) - construct matrix from column vectors
// ============================================================================

mat3 test_mat3_from_vec3_columns() {
    // Constructor mat3(vec3, vec3, vec3) - each vec3 becomes a column
    vec3 col0 = vec3(1.0, 2.0, 3.0);
    vec3 col1 = vec3(4.0, 5.0, 6.0);
    vec3 col2 = vec3(7.0, 8.0, 9.0);
    return mat3(col0, col1, col2);
}

// run: test_mat3_from_vec3_columns() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_from_vec3_expressions() {
    return mat3(vec3(1.0, 2.0, 3.0) + vec3(0.5, 0.5, 0.5), vec3(4.0, 5.0, 6.0) * vec3(1.0, 1.0, 1.0), vec3(7.0, 8.0, 9.0));
}

// run: test_mat3_from_vec3_expressions() ~= mat3(1.5, 2.5, 3.5, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_from_vec3_variables() {
    vec3 a = vec3(1.0, 2.0, 3.0);
    vec3 b = vec3(4.0, 5.0, 6.0);
    vec3 c = vec3(7.0, 8.0, 9.0);
    return mat3(a, b, c);
}

// run: test_mat3_from_vec3_variables() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_from_vec3_identity() {
    vec3 col0 = vec3(1.0, 0.0, 0.0);
    vec3 col1 = vec3(0.0, 1.0, 0.0);
    vec3 col2 = vec3(0.0, 0.0, 1.0);
    return mat3(col0, col1, col2);
}

// run: test_mat3_from_vec3_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_from_vec3_zero() {
    vec3 zero = vec3(0.0, 0.0, 0.0);
    return mat3(zero, zero, zero);
}

// run: test_mat3_from_vec3_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat3 test_mat3_from_vec3_negative() {
    return mat3(vec3(-1.0, -2.0, -3.0), vec3(-4.0, -5.0, -6.0), vec3(-7.0, -8.0, -9.0));
}

// run: test_mat3_from_vec3_negative() ~= mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0)

mat3 test_mat3_from_vec3_mixed() {
    vec3 col0 = vec3(1.0, -2.0, 3.0);
    vec3 col1 = vec3(4.5, 5.5, 6.5);
    vec3 col2 = vec3(7.0, 8.0, 9.0);
    return mat3(col0, col1, col2);
}

// run: test_mat3_from_vec3_mixed() ~= mat3(1.0, -2.0, 3.0, 4.5, 5.5, 6.5, 7.0, 8.0, 9.0)

mat3 test_mat3_from_vec3_in_assignment() {
    mat3 result;
    result = mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0));
    return result;
}

// run: test_mat3_from_vec3_in_assignment() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)

mat3 test_mat3_from_vec3_function_result() {
    return mat3(normalize(vec3(1.0, 2.0, 3.0)), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0));
}

// run: test_mat3_from_vec3_function_result() ~= mat3(0.26726123690605164, 0.5345224738121033, 0.8017836809158325, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)




