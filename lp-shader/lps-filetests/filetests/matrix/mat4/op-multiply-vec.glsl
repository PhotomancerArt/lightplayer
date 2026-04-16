// test run

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
    // Column-major: Col0=(1,2,3,4), Col1=(5,6,7,8), Col2=(9,10,11,12), Col3=(13,14,15,16)
    // Result: [1+5+9+13, 2+6+10+14, 3+7+11+15, 4+8+12+16] = [28, 32, 36, 40]
    return m * v;
}

// run: test_mat4_multiply_vec4_simple() ~= vec4(28.0, 32.0, 36.0, 40.0)

vec4 test_mat4_multiply_vec4_variables() {
    mat4 m = mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    vec4 v = vec4(2.5, 3.7, 4.2, 5.1);
    return m * v;
}

// run: test_mat4_multiply_vec4_variables() ~= vec4(2.5, 3.7, 4.2, 5.1)

vec4 test_mat4_multiply_vec4_expressions() {
    // Column-major: Col0=(1,1,1,1), Col1=(0,1,0,1), Col2=(0,0,1,1), Col3=(0,0,0,1)
    // Result[0] = 1*2+0*3+0*4+0*5 = 2, Result[1] = 1*2+1*3+0*4+0*5 = 5
    // Result[2] = 1*2+0*3+1*4+0*5 = 6, Result[3] = 1*2+1*3+1*4+1*5 = 14
    return mat4(1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0) * vec4(2.0, 3.0, 4.0, 5.0);
}

// run: test_mat4_multiply_vec4_expressions() ~= vec4(2.0, 5.0, 6.0, 14.0)

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
    // Translation matrix (homogeneous coordinates) - column-major layout
    // Translation components (10,20,30) are in the 4th column (w column)
    // Column-major: Col0=(1,0,0,0), Col1=(0,1,0,0), Col2=(0,0,1,0), Col3=(10,20,30,1)
    mat4 m = mat4(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 10.0, 20.0, 30.0, 1.0);
    vec4 v = vec4(1.0, 2.0, 3.0, 1.0); // point in homogeneous coordinates
    return m * v;
}

// run: test_mat4_multiply_vec4_translation() ~= vec4(11.0, 22.0, 33.0, 1.0)




