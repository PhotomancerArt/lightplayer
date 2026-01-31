// test run
// target riscv32.q32

// ============================================================================
// Multiply Vec: mat4 * vec4 -> vec4 (matrix-vector multiplication)
// ============================================================================

vec4 test_mat4_multiply_vec4_identity() {
    // Matrix-vector multiplication with identity matrix
    mat4 m = mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    vec4 v = vec4(3.0, 4.0, 5.0, 6.0);
    return m * v;
}

// run: test_mat4_multiply_vec4_identity() ~= vec4(3.0, 4.0, 5.0, 6.0)

vec4 test_mat4_multiply_vec4_scale() {
    // Scaling transformation
    mat4 m = mat4(2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 5.0); // scale matrix
    vec4 v = vec4(1.0, 1.0, 1.0, 1.0);
    return m * v;
}

// run: test_mat4_multiply_vec4_scale() ~= vec4(2.0, 3.0, 4.0, 5.0)

vec4 test_mat4_multiply_vec4_simple() {
    mat4 m = mat4(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    vec4 v = vec4(1.0, 1.0, 1.0, 1.0);
    // Result: [1+2+3+4, 5+6+7+8, 9+10+11+12, 13+14+15+16] = [10, 26, 42, 58]
    return m * v;
}

// run: test_mat4_multiply_vec4_simple() ~= vec4(10.0, 26.0, 42.0, 58.0) [expect-fail]

vec4 test_mat4_multiply_vec4_variables() {
    mat4 m = mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    vec4 v = vec4(2.5, 3.7, 4.2, 5.1);
    return m * v;
}

// run: test_mat4_multiply_vec4_variables() ~= vec4(2.5, 3.7, 4.2, 5.1)

vec4 test_mat4_multiply_vec4_expressions() {
    return mat4(1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0) * vec4(2.0, 3.0, 4.0, 5.0);
}

// run: test_mat4_multiply_vec4_expressions() ~= vec4(14.0, 10.0, 10.0, 12.0) [expect-fail]

vec4 test_mat4_multiply_vec4_in_assignment() {
    vec4 result;
    mat4 m = mat4(2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0); // uniform scale by 2
    result = m * vec4(1.0, 1.0, 1.0, 1.0);
    return result;
}

// run: test_mat4_multiply_vec4_in_assignment() ~= vec4(2.0, 2.0, 2.0, 2.0)

vec4 test_mat4_multiply_vec4_zero_matrix() {
    mat4 m = mat4(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    vec4 v = vec4(1.0, 2.0, 3.0, 4.0);
    return m * v;
}

// run: test_mat4_multiply_vec4_zero_matrix() ~= vec4(0.0, 0.0, 0.0, 0.0)

vec4 test_mat4_multiply_vec4_negative_values() {
    mat4 m = mat4(-1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0); // reflection over origin
    vec4 v = vec4(2.0, -3.0, 4.0, -5.0);
    return m * v;
}

// run: test_mat4_multiply_vec4_negative_values() ~= vec4(-2.0, 3.0, -4.0, 5.0)

vec4 test_mat4_multiply_vec4_translation() {
    // Translation matrix (homogeneous coordinates)
    mat4 m = mat4(1.0, 0.0, 0.0, 10.0, 0.0, 1.0, 0.0, 20.0, 0.0, 0.0, 1.0, 30.0, 0.0, 0.0, 0.0, 1.0);
    vec4 v = vec4(1.0, 2.0, 3.0, 1.0); // point in homogeneous coordinates
    return m * v;
}

// run: test_mat4_multiply_vec4_translation() ~= vec4(11.0, 22.0, 33.0, 1.0) [expect-fail]




