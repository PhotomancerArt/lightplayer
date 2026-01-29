// test run
// target riscv32.q32

// ============================================================================
// Nested Array Indexing
// ============================================================================

float test_index_nested_2d_float() {
    float arr[3][2] = float[3][2](
        float[2](1.0, 2.0),
        float[2](3.0, 4.0),
        float[2](5.0, 6.0)
    );
    return arr[1][1]; // row 1, column 1
}

// run: test_index_nested_2d_float() ~= 4.0 [expect-fail]

int test_index_nested_2d_int() {
    int arr[2][3] = int[2][3](
        int[3](1, 2, 3),
        int[3](4, 5, 6)
    );
    return arr[0][2]; // row 0, column 2
}

// run: test_index_nested_2d_int() == 3 [expect-fail]

vec2 test_index_nested_vec2_array() {
    vec2 arr[2][2] = vec2[2][2](
        vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0)),
        vec2[2](vec2(5.0, 6.0), vec2(7.0, 8.0))
    );
    return arr[1][0]; // row 1, column 0
}

// run: test_index_nested_vec2_array() ~= vec2(5.0, 6.0) [expect-fail]

float test_index_nested_3d_float() {
    float arr[2][2][2] = float[2][2][2](
        float[2][2](
            float[2](1.0, 2.0),
            float[2](3.0, 4.0)
        ),
        float[2][2](
            float[2](5.0, 6.0),
            float[2](7.0, 8.0)
        )
    );
    return arr[1][0][1]; // layer 1, row 0, column 1
}

// run: test_index_nested_3d_float() ~= 6.0 [expect-fail]

vec3 test_index_nested_vec3_2d() {
    vec3 arr[3][2] = vec3[3][2](
        vec3[2](vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0)),
        vec3[2](vec3(7.0, 8.0, 9.0), vec3(10.0, 11.0, 12.0)),
        vec3[2](vec3(13.0, 14.0, 15.0), vec3(16.0, 17.0, 18.0))
    );
    return arr[2][1]; // row 2, column 1
}

// run: test_index_nested_vec3_2d() ~= vec3(16.0, 17.0, 18.0) [expect-fail]

float test_index_nested_variable_indices() {
    float arr[3][3] = float[3][3](
        float[3](1.0, 2.0, 3.0),
        float[3](4.0, 5.0, 6.0),
        float[3](7.0, 8.0, 9.0)
    );
    int row = 1;
    int col = 2;
    return arr[row][col]; // variable indices
}

// run: test_index_nested_variable_indices() ~= 6.0 [expect-fail]

int test_index_nested_mixed_constant_variable() {
    int arr[2][4] = int[2][4](
        int[4](1, 2, 3, 4),
        int[4](5, 6, 7, 8)
    );
    int col = 3;
    return arr[0][col]; // constant row, variable column
}

// run: test_index_nested_mixed_constant_variable() == 4 [expect-fail]

vec2 test_index_nested_in_expression() {
    vec2 arr[2][2] = vec2[2][2](
        vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0)),
        vec2[2](vec2(5.0, 6.0), vec2(7.0, 8.0))
    );
    return arr[0][1] + arr[1][0]; // vec2(3.0, 4.0) + vec2(5.0, 6.0)
}

// run: test_index_nested_in_expression() ~= vec2(8.0, 10.0) [expect-fail]

float test_index_nested_3d_variable() {
    float arr[2][2][2] = float[2][2][2](
        float[2][2](float[2](1.0, 2.0), float[2](3.0, 4.0)),
        float[2][2](float[2](5.0, 6.0), float[2](7.0, 8.0))
    );
    int x = 1, y = 1, z = 0;
    return arr[x][y][z]; // all variable indices
}

// run: test_index_nested_3d_variable() ~= 7.0 [expect-fail]
