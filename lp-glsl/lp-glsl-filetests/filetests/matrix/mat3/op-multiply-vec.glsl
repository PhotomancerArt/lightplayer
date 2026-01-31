// test run
// target riscv32.q32

// ============================================================================
// Multiply Vec: mat3 * vec3 -> vec3 (matrix-vector multiplication)
// ============================================================================

vec3 test_mat3_multiply_vec3_identity() {
    // Matrix-vector multiplication with identity matrix
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    vec3 v = vec3(3.0, 4.0, 5.0);
    return m * v;
}

// run: test_mat3_multiply_vec3_identity() ~= vec3(3.0, 4.0, 5.0)

vec3 test_mat3_multiply_vec3_scale() {
    // Scaling transformation
    mat3 m = mat3(2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0); // scale by (2, 3, 4)
    vec3 v = vec3(1.0, 1.0, 1.0);
    return m * v;
}

// run: test_mat3_multiply_vec3_scale() ~= vec3(2.0, 3.0, 4.0)

vec3 test_mat3_multiply_vec3_simple() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    vec3 v = vec3(1.0, 2.0, 3.0);
    // Result: [1*1+2*2+3*3, 4*1+5*2+6*3, 7*1+8*2+9*3] = [14, 32, 50]
    return m * v;
}

// run: test_mat3_multiply_vec3_simple() ~= vec3(14.0, 32.0, 50.0) [expect-fail]

vec3 test_mat3_multiply_vec3_rotation_x() {
    // Rotation around X axis by 90 degrees (counterclockwise when looking along +X)
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0);
    vec3 v = vec3(0.0, 1.0, 0.0); // unit vector along Y
    return m * v;
}

// run: test_mat3_multiply_vec3_rotation_x() ~= vec3(0.0, 0.0, 1.0) [expect-fail]

vec3 test_mat3_multiply_vec3_variables() {
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    vec3 v = vec3(2.5, 3.7, 1.2);
    return m * v;
}

// run: test_mat3_multiply_vec3_variables() ~= vec3(2.5, 3.7, 1.2)

vec3 test_mat3_multiply_vec3_expressions() {
    return mat3(1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0) * vec3(2.0, 3.0, 4.0);
}

// run: test_mat3_multiply_vec3_expressions() ~= vec3(5.0, 7.0, 4.0) [expect-fail]

vec3 test_mat3_multiply_vec3_in_assignment() {
    vec3 result;
    mat3 m = mat3(2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0); // uniform scale by 2
    result = m * vec3(1.0, 1.0, 1.0);
    return result;
}

// run: test_mat3_multiply_vec3_in_assignment() ~= vec3(2.0, 2.0, 2.0)

vec3 test_mat3_multiply_vec3_zero_matrix() {
    mat3 m = mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    vec3 v = vec3(1.0, 2.0, 3.0);
    return m * v;
}

// run: test_mat3_multiply_vec3_zero_matrix() ~= vec3(0.0, 0.0, 0.0)

vec3 test_mat3_multiply_vec3_translation_like() {
    // Matrix that acts like translation (though 3x3 matrices can't truly translate in homogeneous coordinates)
    mat3 m = mat3(1.0, 0.0, 1.0, 0.0, 1.0, 2.0, 0.0, 0.0, 1.0); // adds (1,2,0) to XY
    vec3 v = vec3(3.0, 4.0, 1.0);
    return m * v;
}

// run: test_mat3_multiply_vec3_translation_like() ~= vec3(4.0, 6.0, 1.0) [expect-fail]
