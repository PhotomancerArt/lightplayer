// test run
// target riscv32.q32

// ============================================================================
// Matrix Global Types: Global variables of matrix types (mat2, mat3, mat4)
// ============================================================================

mat2 global_mat2;
mat3 global_mat3;
mat4 global_mat4;

mat2 test_type_matrix_mat2() {
    // Global mat2 variable
    global_mat2 = mat2(1.0, 2.0, 3.0, 4.0);
    return global_mat2;
}

// run: test_type_matrix_mat2() ~= mat2(1.0, 2.0, 3.0, 4.0) [expect-fail]

mat3 test_type_matrix_mat3() {
    // Global mat3 variable
    global_mat3 = mat3(1.0, 2.0, 3.0,
                       4.0, 5.0, 6.0,
                       7.0, 8.0, 9.0);
    return global_mat3;
}

// run: test_type_matrix_mat3() ~= mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0) [expect-fail]

mat4 test_type_matrix_mat4() {
    // Global mat4 variable
    global_mat4 = mat4(1.0, 2.0, 3.0, 4.0,
                       5.0, 6.0, 7.0, 8.0,
                       9.0, 10.0, 11.0, 12.0,
                       13.0, 14.0, 15.0, 16.0);
    return global_mat4;
}

// run: test_type_matrix_mat4() ~= mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0) [expect-fail]

mat2 test_type_matrix_identity() {
    // Global mat2 identity matrix
    global_mat2 = mat2(1.0);
    return global_mat2;
}

// run: test_type_matrix_identity() ~= mat2(1.0, 0.0, 0.0, 1.0) [expect-fail]

mat3 test_type_matrix_identity_3() {
    // Global mat3 identity matrix
    global_mat3 = mat3(1.0);
    return global_mat3;
}

// run: test_type_matrix_identity_3() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0) [expect-fail]

mat4 test_type_matrix_identity_4() {
    // Global mat4 identity matrix
    global_mat4 = mat4(1.0);
    return global_mat4;
}

// run: test_type_matrix_identity_4() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0) [expect-fail]

mat2 test_type_matrix_operations() {
    // Matrix operations on global mat2
    global_mat2 = mat2(1.0, 2.0, 3.0, 4.0);
    global_mat2 = global_mat2 * 2.0;
    return global_mat2;
}

// run: test_type_matrix_operations() ~= mat2(2.0, 4.0, 6.0, 8.0) [expect-fail]

mat3 test_type_matrix_multiplication() {
    // Matrix multiplication with global mat3
    global_mat3 = mat3(1.0, 2.0, 3.0,
                       4.0, 5.0, 6.0,
                       7.0, 8.0, 9.0);
    mat3 other = mat3(2.0);
    return global_mat3 * other;
}

// run: test_type_matrix_multiplication() ~= mat3(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0) [expect-fail]

vec2 test_type_matrix_vector_multiply() {
    // Matrix-vector multiplication
    global_mat2 = mat2(1.0, 2.0, 3.0, 4.0);
    vec2 v = vec2(1.0, 1.0);
    return global_mat2 * v;
}

// run: test_type_matrix_vector_multiply() ~= vec2(4.0, 6.0) [expect-fail]

vec3 test_type_matrix_vector_multiply_3() {
    // Matrix-vector multiplication with mat3
    global_mat3 = mat3(1.0);
    vec3 v = vec3(2.0, 3.0, 4.0);
    return global_mat3 * v;
}

// run: test_type_matrix_vector_multiply_3() ~= vec3(2.0, 3.0, 4.0) [expect-fail]

vec4 test_type_matrix_vector_multiply_4() {
    // Matrix-vector multiplication with mat4
    global_mat4 = mat4(1.0);
    vec4 v = vec4(1.0, 2.0, 3.0, 4.0);
    return global_mat4 * v;
}

// run: test_type_matrix_vector_multiply_4() ~= vec4(1.0, 2.0, 3.0, 4.0) [expect-fail]
