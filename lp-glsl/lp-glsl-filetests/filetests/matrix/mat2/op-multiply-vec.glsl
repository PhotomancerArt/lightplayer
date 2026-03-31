// test run

// ============================================================================
// Multiply Vec: mat2 * vec2 -> vec2 (matrix-vector multiplication)
// ============================================================================

vec2 test_mat2_multiply_vec2_identity() {
    // Matrix-vector multiplication with identity matrix
    mat2 m = mat2(1.0, 0.0, 0.0, 1.0);
    vec2 v = vec2(3.0, 4.0);
    return m * v;
}

// run: test_mat2_multiply_vec2_identity() ~= vec2(3.0, 4.0)

vec2 test_mat2_multiply_vec2_scale() {
    // Scaling transformation
    mat2 m = mat2(2.0, 0.0, 0.0, 3.0); // scale by (2, 3)
    vec2 v = vec2(1.0, 1.0);
    return m * v;
}

// run: test_mat2_multiply_vec2_scale() ~= vec2(2.0, 3.0)

vec2 test_mat2_multiply_vec2_rotation() {
    // 90-degree rotation matrix (counterclockwise)
    // Column-major: Col0=(0,-1), Col1=(1,0)
    // For vec2(1,0): Result[0] = 0*1+1*0 = 0, Result[1] = -1*1+0*0 = -1
    mat2 m = mat2(0.0, -1.0, 1.0, 0.0);
    vec2 v = vec2(1.0, 0.0);
    return m * v;
}

// run: test_mat2_multiply_vec2_rotation() ~= vec2(0.0, -1.0)

vec2 test_mat2_multiply_vec2_simple() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    vec2 v = vec2(5.0, 6.0);
    // Column-major: Col0=(1,2), Col1=(3,4)
    // Result[0] = 1*5+3*6 = 23, Result[1] = 2*5+4*6 = 34
    return m * v;
}

// run: test_mat2_multiply_vec2_simple() ~= vec2(23.0, 34.0)

vec2 test_mat2_multiply_vec2_variables() {
    mat2 m = mat2(1.0, 0.0, 0.0, 1.0);
    vec2 v = vec2(2.5, 3.7);
    return m * v;
}

// run: test_mat2_multiply_vec2_variables() ~= vec2(2.5, 3.7)

vec2 test_mat2_multiply_vec2_expressions() {
    // Column-major: Col0=(1,1), Col1=(0,1)
    // Result[0] = 1*2+0*3 = 2, Result[1] = 1*2+1*3 = 5
    return mat2(1.0, 1.0, 0.0, 1.0) * vec2(2.0, 3.0);
}

// run: test_mat2_multiply_vec2_expressions() ~= vec2(2.0, 5.0)

vec2 test_mat2_multiply_vec2_in_assignment() {
    vec2 result;
    mat2 m = mat2(2.0, 0.0, 0.0, 2.0); // uniform scale by 2
    result = m * vec2(1.0, 1.0);
    return result;
}

// run: test_mat2_multiply_vec2_in_assignment() ~= vec2(2.0, 2.0)

vec2 test_mat2_multiply_vec2_zero_matrix() {
    mat2 m = mat2(0.0, 0.0, 0.0, 0.0);
    vec2 v = vec2(1.0, 2.0);
    return m * v;
}

// run: test_mat2_multiply_vec2_zero_matrix() ~= vec2(0.0, 0.0)

vec2 test_mat2_multiply_vec2_negative_values() {
    mat2 m = mat2(-1.0, 0.0, 0.0, -1.0); // reflection over origin
    vec2 v = vec2(2.0, -3.0);
    return m * v;
}

// run: test_mat2_multiply_vec2_negative_values() ~= vec2(-2.0, 3.0)
