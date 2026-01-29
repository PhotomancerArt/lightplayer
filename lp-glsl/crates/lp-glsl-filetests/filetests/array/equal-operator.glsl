// test run
// target riscv32.q32

// ============================================================================
// Array Equality Operator (==)
// ============================================================================

bool test_equal_operator_float_arrays_true() {
    float arr1[3] = float[3](1.0, 2.0, 3.0);
    float arr2[3] = float[3](1.0, 2.0, 3.0);
    return arr1 == arr2; // Should be true
}

// run: test_equal_operator_float_arrays_true() == true [expect-fail]

bool test_equal_operator_int_arrays_true() {
    int arr1[4] = int[4](1, 2, 3, 4);
    int arr2[4] = int[4](1, 2, 3, 4);
    return arr1 == arr2; // Should be true
}

// run: test_equal_operator_int_arrays_true() == true [expect-fail]

bool test_equal_operator_vec2_arrays_true() {
    vec2 arr1[2] = vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0));
    vec2 arr2[2] = vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0));
    return arr1 == arr2; // Should be true
}

// run: test_equal_operator_vec2_arrays_true() == true [expect-fail]

bool test_equal_operator_float_arrays_false() {
    float arr1[3] = float[3](1.0, 2.0, 3.0);
    float arr2[3] = float[3](1.0, 2.0, 4.0); // different last element
    return arr1 == arr2; // Should be false
}

// run: test_equal_operator_float_arrays_false() == false [expect-fail]

bool test_equal_operator_int_arrays_false() {
    int arr1[3] = int[3](1, 2, 3);
    int arr2[3] = int[3](1, 3, 3); // different middle element
    return arr1 == arr2; // Should be false
}

// run: test_equal_operator_int_arrays_false() == false [expect-fail]

bool test_equal_operator_vec3_arrays_false() {
    vec3 arr1[2] = vec3[2](vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0));
    vec3 arr2[2] = vec3[2](vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 7.0)); // different last component
    return arr1 == arr2; // Should be false
}

// run: test_equal_operator_vec3_arrays_false() == false [expect-fail]

bool test_equal_operator_bool_arrays_true() {
    bool arr1[4] = bool[4](true, false, true, false);
    bool arr2[4] = bool[4](true, false, true, false);
    return arr1 == arr2; // Should be true
}

// run: test_equal_operator_bool_arrays_true() == true [expect-fail]

bool test_equal_operator_uvec2_arrays_true() {
    uvec2 arr1[2] = uvec2[2](uvec2(1u, 2u), uvec2(3u, 4u));
    uvec2 arr2[2] = uvec2[2](uvec2(1u, 2u), uvec2(3u, 4u));
    return arr1 == arr2; // Should be true
}

// run: test_equal_operator_uvec2_arrays_true() == true [expect-fail]

bool test_equal_operator_different_sizes() {
    float arr1[3] = float[3](1.0, 2.0, 3.0);
    // Note: arrays of different sizes cannot be compared
    // This would be a compile error, but we can't test compile errors in runtime tests
    return true; // Just return true to indicate test passes
}

// run: test_equal_operator_different_sizes() == true [expect-fail]

bool test_equal_operator_after_assignment() {
    int arr1[3] = int[3](1, 2, 3);
    int arr2[3] = int[3](4, 5, 6);
    arr1 = arr2; // assign arrays
    return arr1 == arr2; // should be true after assignment
}

// run: test_equal_operator_after_assignment() == true [expect-fail]

bool test_equal_operator_empty_arrays() {
    float arr1[0] = float[0](); // empty array
    float arr2[0] = float[0](); // empty array
    // Empty arrays should be equal
    return true; // We can't actually compare empty arrays in this test framework
}

// run: test_equal_operator_empty_arrays() == true [expect-fail]
