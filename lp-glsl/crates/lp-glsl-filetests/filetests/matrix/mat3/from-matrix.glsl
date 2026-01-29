// test run
// target riscv32.q32

// ============================================================================
// From Matrix: mat3(mat2) or mat3(mat4) - construct from other matrices
// ============================================================================

mat3 test_mat3_from_mat2() {
    // Constructor mat3(mat2) - takes mat2 in upper-left, fills rest with identity
    mat2 m2 = mat2(1.0, 2.0, 3.0, 4.0);
    return mat3(m2);
}

// run: test_mat3_from_mat2() ~= mat3(1.0, 2.0, 0.0, 3.0, 4.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_from_mat4() {
    // Constructor mat3(mat4) - takes upper-left 3x3 portion
    mat4 m4 = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    return mat3(m4);
}

// run: test_mat3_from_mat4() ~= mat3(1.0, 2.0, 3.0, 5.0, 6.0, 7.0, 9.0, 10.0, 11.0)

mat3 test_mat3_from_mat3_identity() {
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    return mat3(m);
}

// run: test_mat3_from_mat3_identity() ~= mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_from_mat4_expressions() {
    mat4 m4 = mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), vec4(13.0, 14.0, 15.0, 16.0));
    return mat3(m4 * mat4(2.0));
}

// run: test_mat3_from_mat4_expressions() ~= mat3(2.0, 4.0, 6.0, 10.0, 12.0, 14.0, 18.0, 20.0, 22.0)

mat3 test_mat3_from_mat2_variables() {
    mat2 m2 = mat2(10.0, 20.0, 30.0, 40.0);
    return mat3(m2);
}

// run: test_mat3_from_mat2_variables() ~= mat3(10.0, 20.0, 0.0, 30.0, 40.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_from_mat4_negative() {
    mat4 m4 = mat4(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, -11.0, -12.0, -13.0, -14.0, -15.0, -16.0);
    return mat3(m4);
}

// run: test_mat3_from_mat4_negative() ~= mat3(-1.0, -2.0, -3.0, -5.0, -6.0, -7.0, -9.0, -10.0, -11.0)

mat3 test_mat3_from_mat2_zero() {
    mat2 m2 = mat2(0.0, 0.0, 0.0, 0.0);
    return mat3(m2);
}

// run: test_mat3_from_mat2_zero() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat3 test_mat3_from_mat4_in_assignment() {
    mat3 result;
    mat4 m4 = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    result = mat3(m4);
    return result;
}

// run: test_mat3_from_mat4_in_assignment() ~= mat3(1.0, 2.0, 3.0, 5.0, 6.0, 7.0, 9.0, 10.0, 11.0)




