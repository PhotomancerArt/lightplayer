// test run
// target riscv32.q32

// ============================================================================
// Variable Index Access
// ============================================================================

float test_index_variable_float_array() {
    float arr[5] = float[5](10.0, 20.0, 30.0, 40.0, 50.0);
    int idx = 2;
    return arr[idx]; // variable index
}

// run: test_index_variable_float_array() ~= 30.0 [expect-fail]

int test_index_variable_int_array() {
    int arr[4] = int[4](1, 2, 3, 4);
    int idx = 0;
    return arr[idx]; // variable index
}

// run: test_index_variable_int_array() == 1 [expect-fail]

uint test_index_variable_uint_array() {
    uint arr[3] = uint[3](10u, 20u, 30u);
    uint idx = 1u;
    return arr[idx]; // variable index
}

// run: test_index_variable_uint_array() == 20u [expect-fail]

bool test_index_variable_bool_array() {
    bool arr[4] = bool[4](true, false, true, false);
    int idx = 3;
    return arr[idx]; // variable index
}

// run: test_index_variable_bool_array() == false [expect-fail]

vec2 test_index_variable_vec2_array() {
    vec2 arr[3] = vec2[3](vec2(1.0, 2.0), vec2(3.0, 4.0), vec2(5.0, 6.0));
    int idx = 1;
    return arr[idx]; // variable index
}

// run: test_index_variable_vec2_array() ~= vec2(3.0, 4.0) [expect-fail]

vec3 test_index_variable_vec3_array() {
    vec3 arr[2] = vec3[2](vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0));
    int idx = 0;
    return arr[idx]; // variable index
}

// run: test_index_variable_vec3_array() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

float test_index_variable_computed() {
    float arr[5] = float[5](10.0, 20.0, 30.0, 40.0, 50.0);
    int base = 1;
    int offset = 2;
    return arr[base + offset]; // computed index: 1 + 2 = 3
}

// run: test_index_variable_computed() ~= 40.0 [expect-fail]

int test_index_variable_expression() {
    int arr[4] = int[4](100, 200, 300, 400);
    int x = 2;
    int y = 1;
    return arr[x - y]; // expression index: 2 - 1 = 1
}

// run: test_index_variable_expression() == 200 [expect-fail]

vec2 test_index_variable_in_loop() {
    vec2 arr[3] = vec2[3](vec2(1.0, 1.0), vec2(2.0, 2.0), vec2(3.0, 3.0));
    vec2 sum = vec2(0.0, 0.0);
    for (int i = 0; i < 3; i++) {
        sum += arr[i];
    }
    return sum; // Should be vec2(6.0, 6.0)
}

// run: test_index_variable_in_loop() ~= vec2(6.0, 6.0) [expect-fail]

float test_index_variable_nested_access() {
    float arr[4] = float[4](1.0, 2.0, 3.0, 4.0);
    int indices[2] = int[2](1, 3);
    return arr[indices[0]] + arr[indices[1]]; // 2.0 + 4.0 = 6.0
}

// run: test_index_variable_nested_access() ~= 6.0 [expect-fail]
