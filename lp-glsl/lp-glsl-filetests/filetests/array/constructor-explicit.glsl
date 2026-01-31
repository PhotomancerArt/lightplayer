// test run
// target riscv32.q32

// ============================================================================
// Explicit-size Array Constructors
// ============================================================================

float test_constructor_explicit_float() {
    float arr[3] = float[3](1.0, 2.0, 3.0); // explicit size constructor
    return arr[0]; // Should be 1.0
}

// run: test_constructor_explicit_float() ~= 1.0 [expect-fail]

int test_constructor_explicit_int() {
    int arr[4] = int[4](10, 20, 30, 40); // explicit size constructor
    return arr[2]; // Should be 30
}

// run: test_constructor_explicit_int() == 30 [expect-fail]

uint test_constructor_explicit_uint() {
    uint arr[3] = uint[3](5u, 10u, 15u); // explicit size constructor
    return arr[1]; // Should be 10u
}

// run: test_constructor_explicit_uint() == 10u [expect-fail]

bool test_constructor_explicit_bool() {
    bool arr[3] = bool[3](true, false, true); // explicit size constructor
    return arr[1]; // Should be false
}

// run: test_constructor_explicit_bool() == false [expect-fail]

vec2 test_constructor_explicit_vec2() {
    vec2 arr[2] = vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0)); // explicit size constructor
    return arr[1]; // Should be vec2(3.0, 4.0)
}

// run: test_constructor_explicit_vec2() ~= vec2(3.0, 4.0) [expect-fail]

vec3 test_constructor_explicit_vec3() {
    vec3 arr[2] = vec3[2](vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0)); // explicit size constructor
    return arr[0]; // Should be vec3(1.0, 2.0, 3.0)
}

// run: test_constructor_explicit_vec3() ~= vec3(1.0, 2.0, 3.0) [expect-fail]

ivec2 test_constructor_explicit_ivec2() {
    ivec2 arr[3] = ivec2[3](ivec2(1, 2), ivec2(3, 4), ivec2(5, 6)); // explicit size constructor
    return arr[2]; // Should be ivec2(5, 6)
}

// run: test_constructor_explicit_ivec2() == ivec2(5, 6) [expect-fail]

uvec3 test_constructor_explicit_uvec3() {
    uvec3 arr[2] = uvec3[2](uvec3(1u, 2u, 3u), uvec3(4u, 5u, 6u)); // explicit size constructor
    return arr[1]; // Should be uvec3(4u, 5u, 6u)
}

// run: test_constructor_explicit_uvec3() == uvec3(4u, 5u, 6u) [expect-fail]

bvec4 test_constructor_explicit_bvec4() {
    bvec4 arr[2] = bvec4[2](bvec4(true, false, true, false), bvec4(false, true, false, true)); // explicit size constructor
    return arr[0]; // Should be bvec4(true, false, true, false)
}

// run: test_constructor_explicit_bvec4() == bvec4(true, false, true, false) [expect-fail]

float test_constructor_explicit_mixed_types() {
    float arr[3] = float[3](1, 2.5, 3u); // mixed int/float/uint converted to float
    return arr[1]; // Should be 2.5
}

// run: test_constructor_explicit_mixed_types() ~= 2.5 [expect-fail]

int test_constructor_explicit_scalar_to_vector_array() {
    vec2 arr[2] = vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0)); // vector array constructor
    return int(arr[1].x); // Should be 3
}

// run: test_constructor_explicit_scalar_to_vector_array() == 3 [expect-fail]
