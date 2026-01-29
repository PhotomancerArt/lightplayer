// test run
// target riscv32.q32

// ============================================================================
// Scalar Array Constructors
// ============================================================================

float test_from_scalar_float_array() {
    float arr[3] = float[3](1.0, 2.0, 3.0);
    return arr[0]; // Should be 1.0
}

// run: test_from_scalar_float_array() ~= 1.0 [expect-fail]

int test_from_scalar_int_array() {
    int arr[4] = int[4](10, 20, 30, 40);
    return arr[2]; // Should be 30
}

// run: test_from_scalar_int_array() == 30 [expect-fail]

uint test_from_scalar_uint_array() {
    uint arr[3] = uint[3](1u, 2u, 3u);
    return arr[1]; // Should be 2u
}

// run: test_from_scalar_uint_array() == 2u [expect-fail]

bool test_from_scalar_bool_array() {
    bool arr[3] = bool[3](true, false, true);
    return arr[1]; // Should be false
}

// run: test_from_scalar_bool_array() == false [expect-fail]

float test_from_scalar_mixed_conversions() {
    float arr[3] = float[3](1, 2.5, 3u); // int, float, uint to float
    return arr[1]; // Should be 2.5
}

// run: test_from_scalar_mixed_conversions() ~= 2.5 [expect-fail]

int test_from_scalar_int_from_float() {
    int arr[3] = int[3](1.1, 2.9, 3.0); // float to int (truncation)
    return arr[0] + arr[1] + arr[2]; // 1 + 2 + 3 = 6
}

// run: test_from_scalar_int_from_float() == 6 [expect-fail]

uint test_from_scalar_uint_from_float() {
    uint arr[2] = uint[2](1.7, 2.3); // float to uint (truncation)
    return arr[0] + arr[1]; // 1u + 2u = 3u
}

// run: test_from_scalar_uint_from_float() == 3u [expect-fail]

bool test_from_scalar_bool_from_numeric() {
    bool arr[4] = bool[4](0.0, 1.0, 0, 2); // numeric to bool
    return arr[0] && !arr[1] && arr[2] && arr[3]; // false && true && false && true = false
    // Wait, this is wrong. Let me check: arr[0] is false (0.0), arr[1] is true (1.0), arr[2] is false (0), arr[3] is true (2)
    // false && !true && false && true = false && false && false && true = false
}

// run: test_from_scalar_bool_from_numeric() == false [expect-fail]

float test_from_scalar_single_element() {
    float arr[1] = float[1](42.0);
    return arr[0]; // Should be 42.0
}

// run: test_from_scalar_single_element() ~= 42.0 [expect-fail]

int test_from_scalar_zero_size() {
    // Zero-sized arrays are not allowed, but let's test with size 1
    int arr[1] = int[1](0);
    return arr[0]; // Should be 0
}

// run: test_from_scalar_zero_size() == 0 [expect-fail]

vec2 test_from_scalar_vector_array() {
    vec2 arr[2] = vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0));
    return arr[1]; // Should be vec2(3.0, 4.0)
}

// run: test_from_scalar_vector_array() ~= vec2(3.0, 4.0) [expect-fail]

float test_from_scalar_expression() {
    float arr[3] = float[3](1.0 + 2.0, 3.0 * 2.0, 9.0 / 3.0);
    return arr[0] + arr[1] + arr[2]; // 3.0 + 6.0 + 3.0 = 12.0
}

// run: test_from_scalar_expression() ~= 12.0 [expect-fail]
