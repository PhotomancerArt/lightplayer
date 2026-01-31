// test run
// target riscv32.q32

// ============================================================================
// Unsized Array Declarations (must be initialized)
// ============================================================================

float test_declare_unsized_float_array() {
    float arr[] = float[](1.0, 2.0, 3.0); // unsized array with initializer
    return arr[0]; // Should be 1.0
}

// run: test_declare_unsized_float_array() ~= 1.0 [expect-fail]

int test_declare_unsized_int_array() {
    int arr[] = int[](10, 20, 30, 40); // unsized array with initializer
    return arr[1]; // Should be 20
}

// run: test_declare_unsized_int_array() == 20 [expect-fail]

uint test_declare_unsized_uint_array() {
    uint arr[] = uint[](5u, 10u, 15u); // unsized array with initializer
    return arr[2]; // Should be 15u
}

// run: test_declare_unsized_uint_array() == 15u [expect-fail]

bool test_declare_unsized_bool_array() {
    bool arr[] = bool[](true, false, true); // unsized array with initializer
    return arr[1]; // Should be false
}

// run: test_declare_unsized_bool_array() == false [expect-fail]

vec2 test_declare_unsized_vec2_array() {
    vec2 arr[] = vec2[](vec2(1.0, 2.0), vec2(3.0, 4.0)); // unsized array with initializer
    return arr[0]; // Should be vec2(1.0, 2.0)
}

// run: test_declare_unsized_vec2_array() ~= vec2(1.0, 2.0) [expect-fail]

vec3 test_declare_unsized_vec3_array() {
    vec3 arr[] = vec3[](vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0));
    return arr[1]; // Should be vec3(4.0, 5.0, 6.0)
}

// run: test_declare_unsized_vec3_array() ~= vec3(4.0, 5.0, 6.0) [expect-fail]

ivec2 test_declare_unsized_ivec2_array() {
    ivec2 arr[] = ivec2[](ivec2(1, 2), ivec2(3, 4), ivec2(5, 6));
    return arr[2]; // Should be ivec2(5, 6)
}

// run: test_declare_unsized_ivec2_array() == ivec2(5, 6) [expect-fail]

bvec3 test_declare_unsized_bvec3_array() {
    bvec3 arr[] = bvec3[](bvec3(true, false, true), bvec3(false, true, false));
    return arr[0]; // Should be bvec3(true, false, true)
}

// run: test_declare_unsized_bvec3_array() == bvec3(true, false, true) [expect-fail]

float test_declare_unsized_empty_array() {
    float arr[] = float[](); // empty unsized array
    return float(arr.length()); // Should be 0.0 (length of empty array)
}

// run: test_declare_unsized_empty_array() == 0.0 [expect-fail]

int test_declare_unsized_single_element() {
    int arr[] = int[](42); // unsized array with single element
    return arr[0]; // Should be 42
}

// run: test_declare_unsized_single_element() == 42 [expect-fail]
