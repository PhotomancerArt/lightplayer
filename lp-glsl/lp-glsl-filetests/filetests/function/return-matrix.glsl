// test run
// target riscv32.q32

// ============================================================================
// Matrix Return Types: mat2, mat3, mat4
// ============================================================================

mat2 get_identity2() {
    return mat2(1.0);
}

mat2 test_return_mat2_simple() {
    // Return mat2 value
    return get_identity2();
}

// run: test_return_mat2_simple() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat3 get_identity3() {
    return mat3(1.0);
}

mat3 test_return_mat3_simple() {
    // Return mat3 value
    return get_identity3();
}

// run: test_return_mat3_simple() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat4 get_identity4() {
    return mat4(1.0);
}

mat4 test_return_mat4_simple() {
    // Return mat4 value
    return get_identity4();
}

// run: test_return_mat4_simple() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat2 get_matrix2() {
    return mat2(vec2(1.0, 2.0), vec2(3.0, 4.0));
}

mat2 test_return_mat2_explicit() {
    // Return explicitly constructed mat2
    return get_matrix2();
}

// run: test_return_mat2_explicit() ~= mat2(vec2(1.0, 2.0), vec2(3.0, 4.0))

mat3 get_matrix3() {
    return mat3(vec3(1.0, 0.0, 0.0),
               vec3(0.0, 1.0, 0.0),
               vec3(0.0, 0.0, 1.0));
}

mat3 test_return_mat3_explicit() {
    // Return explicitly constructed mat3
    return get_matrix3();
}

// run: test_return_mat3_explicit() ~= mat3(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0))

mat4 get_matrix4() {
    return mat4(vec4(1.0, 0.0, 0.0, 0.0),
               vec4(0.0, 1.0, 0.0, 0.0),
               vec4(0.0, 0.0, 1.0, 0.0),
               vec4(0.0, 0.0, 0.0, 1.0));
}

mat4 test_return_mat4_explicit() {
    // Return explicitly constructed mat4
    return get_matrix4();
}

// run: test_return_mat4_explicit() ~= mat4(vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.0, 0.0, 0.0, 1.0))

mat2 add_matrices(mat2 a, mat2 b) {
    return a + b;
}

mat2 test_return_mat2_operations() {
    // Return result of matrix operations
    mat2 m1 = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 m2 = mat2(0.5, 1.5, 2.5, 3.5);
    return add_matrices(m1, m2);
}

// run: test_return_mat2_operations() ~= mat2(1.5, 3.5, 5.5, 7.5)

mat3 get_transposed(mat3 m) {
    return transpose(m);
}

mat3 test_return_mat3_transpose() {
    // Return transposed matrix
    mat3 m = mat3(vec3(1.0, 2.0, 3.0),
                 vec3(4.0, 5.0, 6.0),
                 vec3(7.0, 8.0, 9.0));
    return get_transposed(m);
}

// run: test_return_mat3_transpose() ~= mat3(vec3(1.0, 4.0, 7.0), vec3(2.0, 5.0, 8.0), vec3(3.0, 6.0, 9.0))

mat4 get_scaling_matrix(float scale) {
    return mat4(scale, 0.0, 0.0, 0.0,
               0.0, scale, 0.0, 0.0,
               0.0, 0.0, scale, 0.0,
               0.0, 0.0, 0.0, 1.0);
}

mat4 test_return_mat4_scaling() {
    // Return scaling matrix
    return get_scaling_matrix(2.0);
}

// run: test_return_mat4_scaling() ~= mat4(2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0)
