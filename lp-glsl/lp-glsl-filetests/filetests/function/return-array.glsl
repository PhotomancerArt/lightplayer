// test run
// target riscv32.q32

// ============================================================================
// Array Return Types: Arrays must be explicitly sized
// ============================================================================

float[3] get_float_array() {
    return float[3](1.0, 2.0, 3.0);
}

float[3] test_return_array_float() {
    // Return float array
    return get_float_array();
}

// run: test_return_array_float() ~= float[3](1.0, 2.0, 3.0) [expect-fail]

int[2] get_int_array() {
    return int[2](10, 20);
}

int[2] test_return_array_int() {
    // Return int array
    return get_int_array();
}

// run: test_return_array_int() == int[2](10, 20) [expect-fail]

vec2[2] get_vector_array() {
    return vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0));
}

vec2[2] test_return_array_vector() {
    // Return vector array
    return get_vector_array();
}

// run: test_return_array_vector() ~= vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0)) [expect-fail]

float[4] generate_sequence(float start, float step) {
    return float[4](start, start + step, start + 2.0 * step, start + 3.0 * step);
}

float[4] test_return_array_calculated() {
    // Return calculated array values
    return generate_sequence(1.0, 0.5);
}

// run: test_return_array_calculated() ~= float[4](1.0, 1.5, 2.0, 2.5) [expect-fail]

int[3] double_values(int[3] arr) {
    return int[3](arr[0] * 2, arr[1] * 2, arr[2] * 2);
}

int[3] test_return_array_processed() {
    // Return processed array
    int[3] input = int[3](1, 2, 3);
    return double_values(input);
}

// run: test_return_array_processed() == int[3](2, 4, 6) [expect-fail]

float[5] reverse_array(float[5] arr) {
    return float[5](arr[4], arr[3], arr[2], arr[1], arr[0]);
}

float[5] test_return_array_reverse() {
    // Return reversed array
    float[5] input = float[5](1.0, 2.0, 3.0, 4.0, 5.0);
    return reverse_array(input);
}

// run: test_return_array_reverse() ~= float[5](5.0, 4.0, 3.0, 2.0, 1.0) [expect-fail]

bool[3] get_bool_array() {
    return bool[3](true, false, true);
}

bool[3] test_return_array_bool() {
    // Return boolean array
    return get_bool_array();
}

// run: test_return_array_bool() == bool[3](true, false, true) [expect-fail]

float[2] sum_arrays(float[2] a, float[2] b) {
    return float[2](a[0] + b[0], a[1] + b[1]);
}

float[2] test_return_array_sum() {
    // Return array with sum operation
    float[2] arr1 = float[2](1.0, 2.0);
    float[2] arr2 = float[2](3.0, 4.0);
    return sum_arrays(arr1, arr2);
}

// run: test_return_array_sum() ~= float[2](4.0, 6.0) [expect-fail]

mat2[2] get_matrix_array() {
    return mat2[2](mat2(1.0), mat2(2.0));
}

mat2[2] test_return_array_matrix() {
    // Return matrix array
    return get_matrix_array();
}

// run: test_return_array_matrix() ~= mat2[2](mat2(1.0), mat2(2.0)) [expect-fail]
