// test run
// target riscv32.q32

// ============================================================================
// Element Assignment
// ============================================================================

float test_assign_element_float_array() {
    float arr[3] = float[3](1.0, 2.0, 3.0);
    arr[1] = 99.0; // assign to second element
    return arr[1]; // Should be 99.0
}

// run: test_assign_element_float_array() ~= 99.0 [expect-fail]

int test_assign_element_int_array() {
    int arr[4] = int[4](10, 20, 30, 40);
    arr[0] = 100; // assign to first element
    return arr[0]; // Should be 100
}

// run: test_assign_element_int_array() == 100 [expect-fail]

uint test_assign_element_uint_array() {
    uint arr[3] = uint[3](1u, 2u, 3u);
    arr[2] = 99u; // assign to third element
    return arr[2]; // Should be 99u
}

// run: test_assign_element_uint_array() == 99u [expect-fail]

bool test_assign_element_bool_array() {
    bool arr[3] = bool[3](true, false, true);
    arr[1] = true; // assign to second element
    return arr[1]; // Should be true
}

// run: test_assign_element_bool_array() == true [expect-fail]

vec2 test_assign_element_vec2_array() {
    vec2 arr[2] = vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0));
    arr[0] = vec2(10.0, 20.0); // assign to first element
    return arr[0]; // Should be vec2(10.0, 20.0)
}

// run: test_assign_element_vec2_array() ~= vec2(10.0, 20.0) [expect-fail]

vec3 test_assign_element_vec3_array() {
    vec3 arr[3] = vec3[3](vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0));
    arr[1] = vec3(100.0, 200.0, 300.0); // assign to second element
    return arr[1]; // Should be vec3(100.0, 200.0, 300.0)
}

// run: test_assign_element_vec3_array() ~= vec3(100.0, 200.0, 300.0) [expect-fail]

ivec2 test_assign_element_ivec2_array() {
    ivec2 arr[2] = ivec2[2](ivec2(1, 2), ivec2(3, 4));
    arr[1] = ivec2(30, 40); // assign to second element
    return arr[1]; // Should be ivec2(30, 40)
}

// run: test_assign_element_ivec2_array() == ivec2(30, 40) [expect-fail]

float test_assign_element_verify_unchanged() {
    float arr[4] = float[4](1.0, 2.0, 3.0, 4.0);
    arr[1] = 99.0; // change second element
    return arr[0] + arr[2] + arr[3]; // other elements unchanged: 1.0 + 3.0 + 4.0 = 8.0
}

// run: test_assign_element_verify_unchanged() ~= 8.0 [expect-fail]

vec2 test_assign_element_variable_index() {
    vec2 arr[3] = vec2[3](vec2(1.0, 1.0), vec2(2.0, 2.0), vec2(3.0, 3.0));
    int idx = 2;
    arr[idx] = vec2(99.0, 88.0); // variable index assignment
    return arr[2]; // Should be vec2(99.0, 88.0)
}

// run: test_assign_element_variable_index() ~= vec2(99.0, 88.0) [expect-fail]

int test_assign_element_multiple() {
    int arr[5] = int[5](1, 2, 3, 4, 5);
    arr[0] = 10;
    arr[2] = 30;
    arr[4] = 50;
    return arr[0] + arr[1] + arr[2] + arr[3] + arr[4]; // 10 + 2 + 30 + 4 + 50 = 96
}

// run: test_assign_element_multiple() == 96 [expect-fail]
