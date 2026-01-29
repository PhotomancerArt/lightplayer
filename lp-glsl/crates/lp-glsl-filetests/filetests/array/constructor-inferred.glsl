// test run
// target riscv32.q32

// ============================================================================
// Inferred-size Array Constructors
// ============================================================================

float test_constructor_inferred_float() {
    float arr[] = float[](1.0, 2.0, 3.0, 4.0, 5.0); // inferred size constructor
    return arr[3]; // Should be 4.0
}

// run: test_constructor_inferred_float() ~= 4.0 [expect-fail]

int test_constructor_inferred_int() {
    int arr[] = int[](10, 20, 30); // inferred size constructor
    return arr[1]; // Should be 20
}

// run: test_constructor_inferred_int() == 20 [expect-fail]

uint test_constructor_inferred_uint() {
    uint arr[] = uint[](1u, 2u, 3u, 4u, 5u, 6u); // inferred size constructor
    return arr[4]; // Should be 5u
}

// run: test_constructor_inferred_uint() == 5u [expect-fail]

bool test_constructor_inferred_bool() {
    bool arr[] = bool[](true, false, true, false, true); // inferred size constructor
    return arr[2]; // Should be true
}

// run: test_constructor_inferred_bool() == true [expect-fail]

vec2 test_constructor_inferred_vec2() {
    vec2 arr[] = vec2[](vec2(1.0, 2.0), vec2(3.0, 4.0), vec2(5.0, 6.0)); // inferred size constructor
    return arr[1]; // Should be vec2(3.0, 4.0)
}

// run: test_constructor_inferred_vec2() ~= vec2(3.0, 4.0) [expect-fail]

vec3 test_constructor_inferred_vec3() {
    vec3 arr[] = vec3[](vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0)); // inferred size constructor
    return arr[0]; // Should be vec3(1.0, 2.0, 3.0)
}

// run: test_constructor_inferred_vec3() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

ivec3 test_constructor_inferred_ivec3() {
    ivec3 arr[] = ivec3[](ivec3(1, 2, 3), ivec3(4, 5, 6), ivec3(7, 8, 9)); // inferred size constructor
    return arr[2]; // Should be ivec3(7, 8, 9)
}

// run: test_constructor_inferred_ivec3() == ivec3(7, 8, 9) [expect-fail]

uvec4 test_constructor_inferred_uvec4() {
    uvec4 arr[] = uvec4[](uvec4(1u, 2u, 3u, 4u), uvec4(5u, 6u, 7u, 8u)); // inferred size constructor
    return arr[1]; // Should be uvec4(5u, 6u, 7u, 8u)
}

// run: test_constructor_inferred_uvec4() == uvec4(5u, 6u, 7u, 8u) [expect-fail]

bvec2 test_constructor_inferred_bvec2() {
    bvec2 arr[] = bvec2[](bvec2(true, false), bvec2(false, true), bvec2(true, true)); // inferred size constructor
    return arr[0]; // Should be bvec2(true, false)
}

// run: test_constructor_inferred_bvec2() == bvec2(true, false) [expect-fail]

float test_constructor_inferred_single_element() {
    float arr[] = float[](42.0); // inferred size with single element
    return arr[0]; // Should be 42.0
}

// run: test_constructor_inferred_single_element() ~= 42.0 [expect-fail]

int test_constructor_inferred_empty() {
    int arr[] = int[](); // inferred size empty array
    return int(arr.length()); // Should be 0
}

// run: test_constructor_inferred_empty() == 0 [expect-fail]

vec2 test_constructor_inferred_mixed_conversions() {
    vec2 arr[] = vec2[](vec2(1, 2.5), vec2(3.0, 4u)); // mixed type conversions
    return arr[1]; // Should be vec2(3.0, 4.0)
}

// run: test_constructor_inferred_mixed_conversions() ~= vec2(3.0, 4.0) [expect-fail]
