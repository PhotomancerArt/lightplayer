// test run
// target riscv32.q32

// Phase 4: Vector/Matrix Element Arrays - Arrays of vectors and matrices with component access

// Test 1: Vector array declaration and assignment
int test_vector_array_declaration() {
    vec4 arr[2];
    arr[0] = vec4(1.0, 2.0, 3.0, 4.0);
    arr[1] = vec4(5.0, 6.0, 7.0, 8.0);
    return int(arr[0].x + arr[1].x); // Should be 1 + 5 = 6
}
// run: test_vector_array_declaration() == 6

// Test 2: Vector array component access (.x, .y, .z, .w)
int test_vector_component_access() {
    vec4 arr[3];
    arr[0] = vec4(10.0, 20.0, 30.0, 40.0);
    arr[1] = vec4(50.0, 60.0, 70.0, 80.0);
    arr[2] = vec4(90.0, 100.0, 110.0, 120.0);

    float x = arr[0].x; // 10.0
    float y = arr[1].y; // 60.0
    float z = arr[2].z; // 110.0
    float w = arr[0].w; // 40.0

    return int(x + y + z + w); // Should be 10 + 60 + 110 + 40 = 220
}
// run: test_vector_component_access() == 220

// Test 3: Matrix array declaration and assignment
int test_matrix_array_declaration() {
    mat2 mats[2];
    mats[0] = mat2(1.0);
    mats[1] = mat2(3.0);
    return int(mats[0][0][0] + mats[1][0][0]); // Should be 1 + 3 = 4
}
// run: test_matrix_array_declaration() == 4

// Test 4: Matrix array element access [row][col]
int test_matrix_element_access() {
    mat3 mats[2];
    mats[0] = mat3(1.0);
    mats[1] = mat3(2.0);

    float a = mats[0][0][0]; // 1.0 (diagonal element)
    float b = mats[0][1][1]; // 1.0 (diagonal element)
    float c = mats[1][0][0]; // 2.0 (diagonal element)
    float d = mats[1][2][2]; // 2.0 (diagonal element)

    return int(a + b + c + d); // Should be 1 + 1 + 2 + 2 = 6
}
// run: test_matrix_element_access() == 6

// Test 5: Mixed vector and matrix array operations
int test_mixed_vector_matrix_arrays() {
    vec3 vecs[2];
    vecs[0] = vec3(1.0, 2.0, 3.0);
    vecs[1] = vec3(4.0, 5.0, 6.0);

    mat2 mats[2];
    mats[0] = mat2(1.0);
    mats[1] = mat2(2.0);

    float vec_sum = vecs[0].x + vecs[0].y + vecs[0].z; // 1 + 2 + 3 = 6
    float mat_sum = mats[0][0][0] + mats[1][0][0];     // 1 + 2 = 3

    return int(vec_sum + mat_sum); // Should be 6 + 3 = 9
}
// run: test_mixed_vector_matrix_arrays() == 9

// Phase 4 integration test: Arrays of vectors and matrices with component access
int phase4() {
    // Array of vectors
    vec4 arr[3];
    arr[0] = vec4(1.0, 2.0, 3.0, 4.0);
    arr[1] = vec4(5.0, 6.0, 7.0, 8.0);
    arr[2] = vec4(9.0, 10.0, 11.0, 12.0);

    // Component access
    float x = arr[0].x; // 1.0
    float y = arr[1].y; // 6.0
    float z = arr[2].z; // 11.0

    // Array of matrices
    mat3 mats[2];
    mats[0] = mat3(1.0);
    mats[1] = mat3(2.0);

    // Matrix element access
    float m = mats[0][0][0]; // 1.0
    float n = mats[1][1][1]; // 2.0

    return int(x + y + z + m + n); // 1 + 6 + 11 + 1 + 2 = 21
}
// run: phase4() == 21

