// test run
// target riscv32.q32

// ============================================================================
// Constant Index Access
// ============================================================================

float test_index_constant_float_array() {
    float arr[5] = float[5](10.0, 20.0, 30.0, 40.0, 50.0);
    return arr[0]; // first element
}

// run: test_index_constant_float_array() ~= 10.0 [expect-fail]

int test_index_constant_int_array() {
    int arr[4] = int[4](1, 2, 3, 4);
    return arr[1]; // second element
}

// run: test_index_constant_int_array() == 2 [expect-fail]

uint test_index_constant_uint_array() {
    uint arr[3] = uint[3](10u, 20u, 30u);
    return arr[2]; // third element
}

// run: test_index_constant_uint_array() == 30u [expect-fail]

bool test_index_constant_bool_array() {
    bool arr[4] = bool[4](true, false, true, false);
    return arr[1]; // second element
}

// run: test_index_constant_bool_array() == false [expect-fail]

vec2 test_index_constant_vec2_array() {
    vec2 arr[3] = vec2[3](vec2(1.0, 2.0), vec2(3.0, 4.0), vec2(5.0, 6.0));
    return arr[2]; // third element
}

// run: test_index_constant_vec2_array() ~= vec2(5.0, 6.0) [expect-fail]

vec3 test_index_constant_vec3_array() {
    vec3 arr[2] = vec3[2](vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0));
    return arr[0]; // first element
}

// run: test_index_constant_vec3_array() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

ivec3 test_index_constant_ivec3_array() {
    ivec3 arr[3] = ivec3[3](ivec3(1, 2, 3), ivec3(4, 5, 6), ivec3(7, 8, 9));
    return arr[1]; // second element
}

// run: test_index_constant_ivec3_array() == ivec3(4, 5, 6) [expect-fail]

uvec4 test_index_constant_uvec4_array() {
    uvec4 arr[2] = uvec4[2](uvec4(1u, 2u, 3u, 4u), uvec4(5u, 6u, 7u, 8u));
    return arr[1]; // second element
}

// run: test_index_constant_uvec4_array() == uvec4(5u, 6u, 7u, 8u) [expect-fail]

bvec2 test_index_constant_bvec2_array() {
    bvec2 arr[3] = bvec2[3](bvec2(true, false), bvec2(false, true), bvec2(true, true));
    return arr[0]; // first element
}

// run: test_index_constant_bvec2_array() == bvec2(true, false) [expect-fail]

float test_index_constant_all_indices() {
    float arr[5] = float[5](100.0, 200.0, 300.0, 400.0, 500.0);
    float sum = 0.0;
    sum += arr[0]; // 100.0
    sum += arr[1]; // 200.0
    sum += arr[2]; // 300.0
    sum += arr[3]; // 400.0
    sum += arr[4]; // 500.0
    return sum; // Should be 1500.0
}

// run: test_index_constant_all_indices() ~= 1500.0 [expect-fail]

int test_index_constant_repeated_access() {
    int arr[3] = int[3](5, 10, 15);
    return arr[1] + arr[2] + arr[0]; // 10 + 15 + 5 = 30
}

// run: test_index_constant_repeated_access() == 30 [expect-fail]
