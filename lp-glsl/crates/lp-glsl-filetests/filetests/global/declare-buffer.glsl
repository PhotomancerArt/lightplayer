// test run
// target riscv32.q32

// ============================================================================
// Buffer Global Declarations: Global variables with buffer qualifier
// ============================================================================

buffer DataBlock {
    float data_array[];
};

buffer UniformsBlock {
    vec4 colors[4];
    mat4 transforms[2];
    int counts[8];
};

buffer SingleValuesBlock {
    float single_float;
    vec3 single_vec3;
    mat2 single_mat2;
};

float test_declare_buffer_array() {
    // Buffer global array declaration
    // Note: buffer variables can be read and written
    data_array[0] = 42.0;
    data_array[1] = 24.0;
    return data_array[0] + data_array[1];
}

// run: test_declare_buffer_array() ~= 66.0 [expect-fail]

vec4 test_declare_buffer_structured() {
    // Buffer global structured data
    colors[0] = vec4(1.0, 0.0, 0.0, 1.0);
    colors[1] = vec4(0.0, 1.0, 0.0, 1.0);
    colors[2] = vec4(0.0, 0.0, 1.0, 1.0);
    colors[3] = vec4(1.0, 1.0, 1.0, 1.0);

    return colors[0] + colors[1] + colors[2] + colors[3];
}

// run: test_declare_buffer_structured() ~= vec4(2.0, 2.0, 2.0, 4.0) [expect-fail]

mat4 test_declare_buffer_matrix() {
    // Buffer global matrix array
    transforms[0] = mat4(1.0);
    transforms[1] = mat4(2.0);

    return transforms[0] * transforms[1];
}

// run: test_declare_buffer_matrix() ~= mat4(2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0) [expect-fail]

int test_declare_buffer_int_array() {
    // Buffer global int array
    counts[0] = 10;
    counts[1] = 20;
    counts[2] = 30;

    return counts[0] + counts[1] + counts[2];
}

// run: test_declare_buffer_int_array() == 60 [expect-fail]

float test_declare_buffer_single() {
    // Buffer global single values
    single_float = 3.14;
    single_vec3 = vec3(1.0, 2.0, 3.0);
    single_mat2 = mat2(1.0, 0.0, 0.0, 1.0);

    return single_float + single_vec3.x + single_mat2[0][0];
}

// run: test_declare_buffer_single() ~= 5.14 [expect-fail]
