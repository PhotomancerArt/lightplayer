// test run
// target riscv32.q32

// ============================================================================
// From Vectors: mat4(vec4, vec4, vec4, vec4) - construct matrix from column vectors
// ============================================================================

mat4 test_mat4_from_vec4_columns() {
    // Constructor mat4(vec4, vec4, vec4, vec4) - each vec4 becomes a column
    vec4 col0 = vec4(1.0, 2.0, 3.0, 4.0);
    vec4 col1 = vec4(5.0, 6.0, 7.0, 8.0);
    vec4 col2 = vec4(9.0, 10.0, 11.0, 12.0);
    vec4 col3 = vec4(13.0, 14.0, 15.0, 16.0);
    return mat4(col0, col1, col2, col3);
}

// run: test_mat4_from_vec4_columns() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_from_vec4_expressions() {
    return mat4(vec4(1.0, 2.0, 3.0, 4.0) + vec4(0.5, 0.5, 0.5, 0.5), vec4(5.0, 6.0, 7.0, 8.0) * vec4(1.0, 1.0, 1.0, 1.0), vec4(9.0, 10.0, 11.0, 12.0), vec4(13.0, 14.0, 15.0, 16.0));
}

// run: test_mat4_from_vec4_expressions() ~= mat4(1.5, 2.5, 3.5, 4.5, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_from_vec4_variables() {
    vec4 a = vec4(1.0, 2.0, 3.0, 4.0);
    vec4 b = vec4(5.0, 6.0, 7.0, 8.0);
    vec4 c = vec4(9.0, 10.0, 11.0, 12.0);
    vec4 d = vec4(13.0, 14.0, 15.0, 16.0);
    return mat4(a, b, c, d);
}

// run: test_mat4_from_vec4_variables() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_from_vec4_identity() {
    vec4 col0 = vec4(1.0, 0.0, 0.0, 0.0);
    vec4 col1 = vec4(0.0, 1.0, 0.0, 0.0);
    vec4 col2 = vec4(0.0, 0.0, 1.0, 0.0);
    vec4 col3 = vec4(0.0, 0.0, 0.0, 1.0);
    return mat4(col0, col1, col2, col3);
}

// run: test_mat4_from_vec4_identity() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_from_vec4_zero() {
    vec4 zero = vec4(0.0, 0.0, 0.0, 0.0);
    return mat4(zero, zero, zero, zero);
}

// run: test_mat4_from_vec4_zero() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)

mat4 test_mat4_from_vec4_negative() {
    return mat4(vec4(-1.0, -2.0, -3.0, -4.0), vec4(-5.0, -6.0, -7.0, -8.0), vec4(-9.0, -10.0, -11.0, -12.0), vec4(-13.0, -14.0, -15.0, -16.0));
}

// run: test_mat4_from_vec4_negative() ~= mat4(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, -13.0, -14.0, -15.0, -16.0)

mat4 test_mat4_from_vec4_mixed() {
    vec4 col0 = vec4(1.0, -2.0, 3.0, -4.0);
    vec4 col1 = vec4(5.5, 6.5, 7.5, 8.5);
    vec4 col2 = vec4(9.0, 10.0, 11.0, 12.0);
    vec4 col3 = vec4(13.0, 14.0, 15.0, 16.0);
    return mat4(col0, col1, col2, col3);
}

// run: test_mat4_from_vec4_mixed() ~= mat4(1.0, -2.0, 3.0, -4.0, 5.5, 6.5, 7.5, 8.5, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_from_vec4_in_assignment() {
    mat4 result;
    result = mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), vec4(13.0, 14.0, 15.0, 16.0));
    return result;
}

// run: test_mat4_from_vec4_in_assignment() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)

mat4 test_mat4_from_vec4_function_result() {
    return mat4(normalize(vec4(1.0, 2.0, 3.0, 4.0)), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), vec4(13.0, 14.0, 15.0, 16.0));
}

// run: test_mat4_from_vec4_function_result() ~= mat4(0.18257418274879456, 0.3651483654975891, 0.5477225184440613, 0.7302967309951782, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0)




