// test run
// target riscv32.q32

// ============================================================================
// From Matrix: mat4(mat2) or mat4(mat3) - construct from smaller matrices
// ============================================================================

mat4 test_mat4_from_mat2() {
    // Constructor mat4(mat2) - takes mat2 in upper-left, fills rest with identity
    mat2 m2 = mat2(1.0, 2.0, 3.0, 4.0);
    return mat4(m2);
}

// run: test_mat4_from_mat2() ~= mat4(1.0, 2.0, 0.0, 0.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_from_mat3() {
    // Constructor mat4(mat3) - takes mat3 in upper-left, fills rest with identity
    mat3 m3 = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return mat4(m3);
}

// run: test_mat4_from_mat3() ~= mat4(1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0, 7.0, 8.0, 9.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_from_mat4_identity() {
    mat4 m = mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    return mat4(m);
}

// run: test_mat4_from_mat4_identity() ~= mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_from_mat2_expressions() {
    mat2 m2 = mat2(1.0, 2.0, 3.0, 4.0);
    return mat4(m2 * mat2(2.0));
}

// run: test_mat4_from_mat2_expressions() ~= mat4(2.0, 4.0, 0.0, 0.0, 6.0, 8.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0) [expect-fail]

mat4 test_mat4_from_mat3_variables() {
    mat3 m3 = mat3(10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0);
    return mat4(m3);
}

// run: test_mat4_from_mat3_variables() ~= mat4(10.0, 20.0, 30.0, 0.0, 40.0, 50.0, 60.0, 0.0, 70.0, 80.0, 90.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_from_mat2_negative() {
    mat2 m2 = mat2(-1.0, -2.0, -3.0, -4.0);
    return mat4(m2);
}

// run: test_mat4_from_mat2_negative() ~= mat4(-1.0, -2.0, 0.0, 0.0, -3.0, -4.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_from_mat3_zero() {
    mat3 m3 = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return mat4(m3);
}

// run: test_mat4_from_mat3_zero() ~= mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_from_mat2_in_assignment() {
    mat4 result;
    mat2 m2 = mat2(1.0, 2.0, 3.0, 4.0);
    result = mat4(m2);
    return result;
}

// run: test_mat4_from_mat2_in_assignment() ~= mat4(1.0, 2.0, 0.0, 0.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)

mat4 test_mat4_from_mat3_in_assignment() {
    mat4 result;
    mat3 m3 = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result = mat4(m3);
    return result;
}

// run: test_mat4_from_mat3_in_assignment() ~= mat4(1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0, 7.0, 8.0, 9.0, 0.0, 0.0, 0.0, 0.0, 1.0)




