// test run
// target riscv32.q32

// ============================================================================
// inverse(): Matrix inverse function
// inverse(mat) - returns inverse of matrix
// Undefined if matrix is singular or poorly-conditioned
// ============================================================================

mat2 test_inverse_mat2_identity() {
    // inverse of 2x2 identity matrix
    mat2 m = mat2(1.0, 0.0, 0.0, 1.0);
    return inverse(m);
}

// run: test_inverse_mat2_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_inverse_mat2_simple() {
    // inverse of 2x2 matrix [1,2; 3,4]
    // inverse = 1/(-2) * [-4, 2; -3, 1] = [-2, 1; -1.5, 0.5]
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    return inverse(m);
}

// run: test_inverse_mat2_simple() ~= mat2(-2.0, 1.0, -1.5, 0.5) [expect-fail]

mat3 test_inverse_mat3_identity() {
    // inverse of 3x3 identity matrix
    mat3 m = mat3(1.0);
    return inverse(m);
}

// run: test_inverse_mat3_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_inverse_mat4_identity() {
    // inverse of 4x4 identity matrix
    mat4 m = mat4(1.0);
    return inverse(m);
}

// run: test_inverse_mat4_identity() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat2 test_inverse_mat2_scaling() {
    // inverse of scaling matrix [2,0; 0,3] should be [0.5,0; 0,0.333]
    mat2 m = mat2(2.0, 0.0, 0.0, 3.0);
    return inverse(m);
}

// run: test_inverse_mat2_scaling() ~= mat2(0.5, 0.0, 0.0, 0.3333333333333333)

mat3 test_inverse_mat3_scaling() {
    mat3 m = mat3(2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0);
    return inverse(m);
}

// run: test_inverse_mat3_scaling() ~= mat3(0.5, 0.0, 0.0, 0.0, 0.3333333333333333, 0.0, 0.0, 0.0, 0.25) [expect-fail]

mat4 test_inverse_mat4_scaling() {
    mat4 m = mat4(2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 5.0);
    return inverse(m);
}

// run: test_inverse_mat4_scaling() ~= mat4(0.5, 0.0, 0.0, 0.0, 0.0, 0.3333333333333333, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0, 0.2) [expect-fail]

mat2 test_inverse_mat2_rotation() {
    // inverse of rotation matrix [0,1; -1,0] should be [0,-1; 1,0]
    mat2 m = mat2(0.0, 1.0, -1.0, 0.0);
    return inverse(m);
}

// run: test_inverse_mat2_rotation() ~= mat2(0.0, -1.0, 1.0, 0.0)

mat3 test_inverse_mat3_simple() {
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return inverse(m);
}

// run: test_inverse_mat3_simple() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_inverse_mat4_simple() {
    mat4 m = mat4(2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0);
    return inverse(m);
}

// run: test_inverse_mat4_simple() ~= mat4(0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5)

mat2 test_inverse_mat2_expressions() {
    return inverse(mat2(1.0, 0.0, 0.0, 1.0));
}

// run: test_inverse_mat2_expressions() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat3 test_inverse_mat3_expressions() {
    return inverse(mat3(2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0));
}

// run: test_inverse_mat3_expressions() ~= mat3(0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5)

mat4 test_inverse_mat4_expressions() {
    return inverse(mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0));
}

// run: test_inverse_mat4_expressions() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat2 test_inverse_mat2_variables() {
    mat2 m = mat2(4.0, 2.0, 3.0, 1.0);
    return inverse(m);
}

// run: test_inverse_mat2_variables() ~= mat2(-0.5, 1.0, 1.5, -2.0)

mat3 test_inverse_mat3_variables() {
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0);
    return inverse(m);
}

// run: test_inverse_mat3_variables() ~= mat3(1.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.3333333333333333)

mat4 test_inverse_mat4_variables() {
    mat4 m = mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    return inverse(m);
}

// run: test_inverse_mat4_variables() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)
