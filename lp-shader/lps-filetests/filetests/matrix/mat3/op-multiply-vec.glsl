// test run

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
    // Column-major: Col0=(1,2,3), Col1=(4,5,6), Col2=(7,8,9)
    // Result[0] = 1*1+4*2+7*3 = 30, Result[1] = 2*1+5*2+8*3 = 36, Result[2] = 3*1+6*2+9*3 = 42
    return m * v;
}

// run: test_mat3_multiply_vec3_simple() ~= vec3(30.0, 36.0, 42.0)

vec3 test_mat3_multiply_vec3_rotation_x() {
    // Rotation around X axis by 90 degrees (counterclockwise when looking along +X)
    // Column-major: Col0=(1,0,0), Col1=(0,0,1), Col2=(0,-1,0)
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0);
    vec3 v = vec3(0.0, 1.0, 0.0); // unit vector along Y
    return m * v;
}

// run: test_mat3_multiply_vec3_rotation_x() ~= vec3(0.0, 0.0, 1.0)

vec3 test_mat3_multiply_vec3_variables() {
    mat3 m = mat3(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    vec3 v = vec3(2.5, 3.7, 1.2);
    return m * v;
}

// run: test_mat3_multiply_vec3_variables() ~= vec3(2.5, 3.7, 1.2)

vec3 test_mat3_multiply_vec3_expressions() {
    // Column-major: Col0=(1,1,0), Col1=(0,1,1), Col2=(0,0,1)
    // Result[0] = 1*2+0*3+0*4 = 2, Result[1] = 1*2+1*3+0*4 = 5, Result[2] = 0*2+1*3+1*4 = 7
    return mat3(1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0) * vec3(2.0, 3.0, 4.0);
}

// run: test_mat3_multiply_vec3_expressions() ~= vec3(2.0, 5.0, 7.0)

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
    // Column-major: Col0=(1,0,1), Col1=(0,1,2), Col2=(0,0,1)
    // For vec3(3,4,1): Result[0] = 1*3+0*4+0*1 = 3, Result[1] = 0*3+1*4+0*1 = 4, Result[2] = 1*3+2*4+1*1 = 12
    mat3 m = mat3(1.0, 0.0, 1.0, 0.0, 1.0, 2.0, 0.0, 0.0, 1.0); // adds z to x and 2z to y
    vec3 v = vec3(3.0, 4.0, 1.0);
    return m * v;
}

// run: test_mat3_multiply_vec3_translation_like() ~= vec3(3.0, 4.0, 12.0)
