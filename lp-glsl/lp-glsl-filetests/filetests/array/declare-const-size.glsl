// test run
// target riscv32.q32

// ============================================================================
// Const Int/Uint Variables as Array Sizes
// ============================================================================

// Basic const int as array size
const int SIZE = 5;
float arr[SIZE];

// Basic const uint as array size
const uint U_SIZE = 3u;
int arr2[U_SIZE];

// Const variables in different scopes
float test_local_const() {
    const int LOCAL_SIZE = 4;
    float local_arr[LOCAL_SIZE];
    return 1.0;
}

// run: test_local_const() == 1.0 [expect-fail]

// Multiple arrays using same const
const int COMMON_SIZE = 10;
float arr_a[COMMON_SIZE];
int arr_b[COMMON_SIZE];
vec2 arr_c[COMMON_SIZE];

float test_const_global_arrays() {
    // Test that multiple arrays can use the same const size
    return 1.0;
}

// run: test_const_global_arrays() == 1.0 [expect-fail]

int test_const_uint_size() {
    // Test const uint as array size
    const uint TEST_SIZE = 4u;
    int test_arr[TEST_SIZE];
    return 1;
}

// run: test_const_uint_size() == 1 [expect-fail]

vec2 test_const_vec2_array() {
    const int VEC_SIZE = 3;
    vec2 vec_arr[VEC_SIZE];
    return vec2(1.0, 1.0);
}

// run: test_const_vec2_array() ~= vec2(1.0, 1.0) [expect-fail]

vec3 test_const_vec3_array() {
    const int VEC3_SIZE = 2;
    vec3 vec3_arr[VEC3_SIZE];
    return vec3(1.0, 1.0, 1.0);
}

// run: test_const_vec3_array() ~= vec3(1.0, 1.0, 1.0) [expect-fail]

vec4 test_const_vec4_array() {
    const uint VEC4_SIZE = 3u;
    vec4 vec4_arr[VEC4_SIZE];
    return vec4(1.0, 1.0, 1.0, 1.0);
}

// run: test_const_vec4_array() ~= vec4(1.0, 1.0, 1.0, 1.0) [expect-fail]




