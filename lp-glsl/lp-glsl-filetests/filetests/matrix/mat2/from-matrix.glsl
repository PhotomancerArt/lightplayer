// test run
// target riscv32.q32

// ============================================================================
// From Matrix: mat2(mat3) or mat2(mat4) - construct from larger matrix
// ============================================================================

mat2 test_mat2_from_mat3() {
    // Constructor mat2(mat3) - takes upper-left 2x2 portion
    mat3 m3 = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return mat2(m3);
}

// run: test_mat2_from_mat3() ~= mat2(1.0, 2.0, 4.0, 5.0)

mat2 test_mat2_from_mat4() {
    // Constructor mat2(mat4) - takes upper-left 2x2 portion
    mat4 m4 = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    return mat2(m4);
}

// run: test_mat2_from_mat4() ~= mat2(1.0, 2.0, 5.0, 6.0)

mat2 test_mat2_from_mat2_identity() {
    mat2 m = mat2(1.0, 0.0, 0.0, 1.0);
    return mat2(m);
}

// run: test_mat2_from_mat2_identity() ~= mat2(1.0, 0.0, 0.0, 1.0)

mat2 test_mat2_from_mat3_expressions() {
    mat3 m3 = mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0));
    return mat2(m3 * mat3(2.0));
}

// run: test_mat2_from_mat3_expressions() ~= mat2(2.0, 4.0, 8.0, 10.0)

mat2 test_mat2_from_mat4_variables() {
    mat4 m4 = mat4(10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0, 120.0, 130.0, 140.0, 150.0, 160.0);
    return mat2(m4);
}

// run: test_mat2_from_mat4_variables() ~= mat2(10.0, 20.0, 50.0, 60.0)

mat2 test_mat2_from_mat3_negative() {
    mat3 m3 = mat3(-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0);
    return mat2(m3);
}

// run: test_mat2_from_mat3_negative() ~= mat2(-1.0, -2.0, -4.0, -5.0)

mat2 test_mat2_from_mat4_zero() {
    mat4 m4 = mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    return mat2(m4);
}

// run: test_mat2_from_mat4_zero() ~= mat2(0.0, 0.0, 0.0, 0.0)

mat2 test_mat2_from_mat3_in_assignment() {
    mat2 result;
    mat3 m3 = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    result = mat2(m3);
    return result;
}

// run: test_mat2_from_mat3_in_assignment() ~= mat2(1.0, 2.0, 4.0, 5.0)




