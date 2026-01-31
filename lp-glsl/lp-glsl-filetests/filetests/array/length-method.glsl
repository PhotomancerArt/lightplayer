// test run
// target riscv32.q32

// ============================================================================
// Array Length Method (.length())
// ============================================================================

int test_length_method_float_array() {
    float arr[5] = float[5](1.0, 2.0, 3.0, 4.0, 5.0);
    return arr.length(); // Should be 5
}

// run: test_length_method_float_array() == 5 [expect-fail]

int test_length_method_int_array() {
    int arr[3] = int[3](10, 20, 30);
    return arr.length(); // Should be 3
}

// run: test_length_method_int_array() == 3 [expect-fail]

int test_length_method_vec2_array() {
    vec2 arr[4] = vec2[4](vec2(1.0, 1.0), vec2(2.0, 2.0), vec2(3.0, 3.0), vec2(4.0, 4.0));
    return arr.length(); // Should be 4
}

// run: test_length_method_vec2_array() == 4 [expect-fail]

int test_length_method_bool_array() {
    bool arr[6] = bool[6](true, false, true, false, true, false);
    return arr.length(); // Should be 6
}

// run: test_length_method_bool_array() == 6 [expect-fail]

int test_length_method_uvec3_array() {
    uvec3 arr[2] = uvec3[2](uvec3(1u, 1u, 1u), uvec3(2u, 2u, 2u));
    return arr.length(); // Should be 2
}

// run: test_length_method_uvec3_array() == 2 [expect-fail]

int test_length_method_empty_array() {
    float arr[0] = float[0](); // zero-sized array
    return arr.length(); // Should be 0
}

// run: test_length_method_empty_array() == 0 [expect-fail]

int test_length_method_large_array() {
    int arr[100]; // large array
    return arr.length(); // Should be 100
}

// run: test_length_method_large_array() == 100 [expect-fail]

int test_length_method_in_expression() {
    float arr[7] = float[7](1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
    return int(arr.length() / 2); // 7 / 2 = 3 (integer division)
}

// run: test_length_method_in_expression() == 3 [expect-fail]

int test_length_method_multidimensional() {
    float arr[3][4] = float[3][4](
        float[4](1.0, 1.0, 1.0, 1.0),
        float[4](1.0, 1.0, 1.0, 1.0),
        float[4](1.0, 1.0, 1.0, 1.0)
    );
    return arr.length(); // Should be 3 (outer dimension)
}

// run: test_length_method_multidimensional() == 3 [expect-fail]

int test_length_method_unsized_array() {
    float arr[] = float[](1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
    return arr.length(); // Should be 6
}

// run: test_length_method_unsized_array() == 6 [expect-fail]

float test_length_method_as_float() {
    vec3 arr[3] = vec3[3](vec3(1.0, 1.0, 1.0), vec3(1.0, 1.0, 1.0), vec3(1.0, 1.0, 1.0));
    return float(arr.length()); // Should be 3.0
}

// run: test_length_method_as_float() ~= 3.0 [expect-fail]
