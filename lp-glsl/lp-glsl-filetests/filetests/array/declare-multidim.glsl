// test run
// target riscv32.q32

// ============================================================================
// Multi-dimensional Array Declarations
// ============================================================================

float test_declare_2d_float_array() {
    float arr[3][2]; // 3x2 array of floats
    return 1.0; // Declaration test - no runtime behavior
}

// run: test_declare_2d_float_array() == 1.0 [expect-fail]

int test_declare_2d_int_array() {
    int arr[2][3]; // 2x3 array of ints
    return 1; // Declaration test
}

// run: test_declare_2d_int_array() == 1 [expect-fail]

vec3 test_declare_2d_vec3_array() {
    vec3 arr[2][2]; // 2x2 array of vec3s
    return vec3(1.0, 1.0, 1.0); // Declaration test
}

// run: test_declare_2d_vec3_array() ~= vec3(1.0, 1.0, 1.0) [expect-fail]

float test_declare_3d_float_array() {
    float arr[2][3][4]; // 2x3x4 array of floats
    return 1.0; // Declaration test
}

// run: test_declare_3d_float_array() == 1.0 [expect-fail]

vec4 test_declare_3d_vec4_array() {
    vec4 arr[2][2][2]; // 2x2x2 array of vec4s
    return vec4(1.0, 1.0, 1.0, 1.0); // Declaration test
}

// run: test_declare_3d_vec4_array() ~= vec4(1.0, 1.0, 1.0, 1.0) [expect-fail]

int test_declare_large_2d_array() {
    int arr[10][5]; // 10x5 array of ints
    return 1; // Declaration test
}

// run: test_declare_large_2d_array() == 1 [expect-fail]

float test_declare_uneven_dimensions() {
    float arr[4][6]; // 4x6 array (different dimension sizes)
    return 1.0; // Declaration test
}

// run: test_declare_uneven_dimensions() == 1.0 [expect-fail]

vec2 test_declare_2d_with_initializer() {
    vec2 arr[2][2] = vec2[][](
        vec2[](vec2(1.0, 2.0), vec2(3.0, 4.0)),
        vec2[](vec2(5.0, 6.0), vec2(7.0, 8.0))
    ); // 2x2 array with initializer
    return arr[0][0]; // Should be vec2(1.0, 2.0)
}

// run: test_declare_2d_with_initializer() ~= vec2(1.0, 2.0) [expect-fail]

int test_declare_2d_access_inner() {
    int arr[3][2] = int[][](
        int[](1, 2),
        int[](3, 4),
        int[](5, 6)
    ); // 3x2 array with initializer
    return arr[1][1]; // Should be 4 (second row, second column)
}

// run: test_declare_2d_access_inner() == 4 [expect-fail]

float test_declare_3d_access() {
    float arr[2][2][2] = float[][][](
        float[][](float[](1.0, 2.0), float[](3.0, 4.0)),
        float[][](float[](5.0, 6.0), float[](7.0, 8.0))
    ); // 2x2x2 array with initializer
    return arr[1][0][1]; // Should be 6.0
}

// run: test_declare_3d_access() ~= 6.0 [expect-fail]
