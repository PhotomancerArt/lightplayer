// test run
// target riscv32.q32

// ============================================================================
// Whole Array Assignment
// ============================================================================

float test_assign_whole_float_array() {
    float arr1[3] = float[3](1.0, 2.0, 3.0);
    float arr2[3] = float[3](10.0, 20.0, 30.0);
    arr1 = arr2; // whole array assignment
    return arr1[1]; // Should be 20.0
}

// run: test_assign_whole_float_array() ~= 20.0 [expect-fail]

int test_assign_whole_int_array() {
    int arr1[4] = int[4](1, 2, 3, 4);
    int arr2[4] = int[4](5, 6, 7, 8);
    arr1 = arr2; // whole array assignment
    return arr1[0] + arr1[3]; // 5 + 8 = 13
}

// run: test_assign_whole_int_array() == 13 [expect-fail]

vec2 test_assign_whole_vec2_array() {
    vec2 arr1[2] = vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0));
    vec2 arr2[2] = vec2[2](vec2(10.0, 20.0), vec2(30.0, 40.0));
    arr1 = arr2; // whole array assignment
    return arr1[1]; // Should be vec2(30.0, 40.0)
}

// run: test_assign_whole_vec2_array() ~= vec2(30.0, 40.0) [expect-fail]

vec3 test_assign_whole_vec3_array() {
    vec3 arr1[3] = vec3[3](vec3(1.0, 1.0, 1.0), vec3(2.0, 2.0, 2.0), vec3(3.0, 3.0, 3.0));
    vec3 arr2[3] = vec3[3](vec3(10.0, 10.0, 10.0), vec3(20.0, 20.0, 20.0), vec3(30.0, 30.0, 30.0));
    arr1 = arr2; // whole array assignment
    return arr1[2]; // Should be vec3(30.0, 30.0, 30.0)
}

// run: test_assign_whole_vec3_array() ~= vec3(30.0, 30.0, 30.0) [expect-fail]

ivec3 test_assign_whole_ivec3_array() {
    ivec3 arr1[2] = ivec3[2](ivec3(1, 1, 1), ivec3(2, 2, 2));
    ivec3 arr2[2] = ivec3[2](ivec3(10, 10, 10), ivec3(20, 20, 20));
    arr1 = arr2; // whole array assignment
    return arr1[0]; // Should be ivec3(10, 10, 10)
}

// run: test_assign_whole_ivec3_array() == ivec3(10, 10, 10) [expect-fail]

bvec4 test_assign_whole_bvec4_array() {
    bvec4 arr1[2] = bvec4[2](bvec4(true, true, true, true), bvec4(false, false, false, false));
    bvec4 arr2[2] = bvec4[2](bvec4(false, false, false, false), bvec4(true, true, true, true));
    arr1 = arr2; // whole array assignment
    return arr1[1]; // Should be bvec4(true, true, true, true)
}

// run: test_assign_whole_bvec4_array() == bvec4(true, true, true, true) [expect-fail]

float test_assign_whole_array_expression() {
    float arr1[3] = float[3](1.0, 2.0, 3.0);
    arr1 = float[3](100.0, 200.0, 300.0); // assign from constructor
    return arr1[2]; // Should be 300.0
}

// run: test_assign_whole_array_expression() ~= 300.0 [expect-fail]

vec2 test_assign_whole_array_in_function() {
    vec2 arr1[2] = vec2[2](vec2(1.0, 1.0), vec2(2.0, 2.0));
    vec2 arr2[2] = vec2[2](vec2(5.0, 5.0), vec2(6.0, 6.0));
    arr1 = arr2; // whole array assignment
    return arr1[0] + arr1[1]; // vec2(5.0, 5.0) + vec2(6.0, 6.0) = vec2(11.0, 11.0)
}

// run: test_assign_whole_array_in_function() ~= vec2(11.0, 11.0) [expect-fail]

int test_assign_whole_array_return() {
    int arr[3] = int[3](1, 2, 3);
    int other[3] = int[3](7, 8, 9);
    arr = other; // whole array assignment
    return arr[0] + arr[1] + arr[2]; // 7 + 8 + 9 = 24
}

// run: test_assign_whole_array_return() == 24 [expect-fail]

float test_assign_whole_array_self() {
    float arr[4] = float[4](1.0, 2.0, 3.0, 4.0);
    arr = arr; // self assignment (should be no-op)
    return arr[3]; // Should be 4.0
}

// run: test_assign_whole_array_self() ~= 4.0 [expect-fail]
