// test run
// target riscv32.q32

// ============================================================================
// Array Parameters: Arrays passed to functions
// ============================================================================

float sum_array(float[3] arr) {
    return arr[0] + arr[1] + arr[2];
}

float test_param_array_sum() {
    // Sum elements of float array
    float[3] data = float[3](1.0, 2.0, 3.0);
    return sum_array(data);
}

// run: test_param_array_sum() ~= 6.0 [expect-fail]

void double_elements(inout float[4] arr) {
    arr[0] = arr[0] * 2.0;
    arr[1] = arr[1] * 2.0;
    arr[2] = arr[2] * 2.0;
    arr[3] = arr[3] * 2.0;
}

void test_param_array_modify() {
    // Modify array elements inside function
    float[4] values = float[4](1.0, 2.0, 3.0, 4.0);
    double_elements(values);
    // values should now be [2.0, 4.0, 6.0, 8.0]
}

// run: test_param_array_modify() == 0.0 [expect-fail]

int product(int[3] arr) {
    return arr[0] * arr[1] * arr[2];
}

float test_param_array_int() {
    // Integer array parameters
    int[3] factors = int[3](2, 3, 4);
    return float(product(factors));
}

// run: test_param_array_int() ~= 24.0 [expect-fail]

vec2 sum_vectors_arr(vec2[2] arr) {
    return arr[0] + arr[1];
}

vec2 test_param_array_vector() {
    // Vector array parameters
    vec2[2] vectors = vec2[2](vec2(1.0, 2.0), vec2(3.0, 4.0));
    return sum_vectors_arr(vectors);
}

// run: test_param_array_vector() ~= vec2(4.0, 6.0) [expect-fail]

float average(const float[5] arr) {
    return (arr[0] + arr[1] + arr[2] + arr[3] + arr[4]) / 5.0;
}

float test_param_array_const() {
    // Const array parameters
    float[5] data = float[5](10.0, 20.0, 30.0, 40.0, 50.0);
    return average(data);
}

// run: test_param_array_const() ~= 30.0 [expect-fail]

void fill_sequence(out int[4] arr) {
    for (int i = 0; i < 4; i++) {
        arr[i] = i + 1;
    }
}

void test_param_array_out() {
    // Out array parameters
    int[4] sequence;
    fill_sequence(sequence);
    // sequence should be [1, 2, 3, 4]
}

// run: test_param_array_out() == 0.0 [expect-fail]

void increment_elements(inout float[3] arr, float amount) {
    arr[0] = arr[0] + amount;
    arr[1] = arr[1] + amount;
    arr[2] = arr[2] + amount;
}

float test_param_array_inout() {
    // Inout array parameters
    float[3] values = float[3](1.0, 2.0, 3.0);
    increment_elements(values, 10.0);
    return values[0] + values[1] + values[2]; // 11 + 12 + 13 = 36
}

// run: test_param_array_inout() ~= 36.0 [expect-fail]

float sum2_arr(float[2] arr) {
    return arr[0] + arr[1];
}

float sum3_arr(float[3] arr) {
    return arr[0] + arr[1] + arr[2];
}

float test_param_array_different_sizes() {
    // Different array sizes as overloads
    float[2] arr2 = float[2](5.0, 6.0);
    float[3] arr3 = float[3](1.0, 2.0, 3.0);
    return sum2_arr(arr2) + sum3_arr(arr3); // 11 + 6 = 17
}

// run: test_param_array_different_sizes() ~= 17.0 [expect-fail]

bool all_true_arr(bool[3] arr) {
    return arr[0] && arr[1] && arr[2];
}

bool test_param_array_bool() {
    // Boolean array parameters
    bool[3] flags = bool[3](true, true, false);
    return all_true_arr(flags);
}

// run: test_param_array_bool() == false [expect-fail]
