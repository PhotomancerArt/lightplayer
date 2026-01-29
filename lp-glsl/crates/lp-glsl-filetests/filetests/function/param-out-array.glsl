// test run
// target riscv32.q32

// ============================================================================
// Out Parameters with Arrays: Array elements as out/inout parameters
// ============================================================================

void set_array_element(out float[3] arr, int idx) {
    arr[idx] = 42.0;
}

float test_param_out_array_element() {
    // Out parameter with array element access
    float[3] arr;
    set_array_element(arr, 1);
    return arr[1];
}

// run: test_param_out_array_element() ~= 42.0

void set_first_element(out float[3] arr) {
    arr[0] = 10.0;
}

float test_param_out_array_first() {
    // Out parameter with array, setting first element
    float[3] arr;
    set_first_element(arr);
    return arr[0];
}

// run: test_param_out_array_first() ~= 10.0

void modify_array_element(inout float[3] arr, int idx) {
    arr[idx] = arr[idx] * 2.0;
}

float test_param_inout_array_element() {
    // Inout parameter with array element
    float[3] arr;
    arr[1] = 5.0;
    modify_array_element(arr, 1);
    return arr[1];
}

// run: test_param_inout_array_element() ~= 10.0

void set_multiple_elements(out float[3] arr) {
    arr[0] = 1.0;
    arr[1] = 2.0;
    arr[2] = 3.0;
}

float test_param_out_array_multiple() {
    // Out parameter with array, setting multiple elements
    float[3] arr;
    set_multiple_elements(arr);
    return arr[0] + arr[1] + arr[2];
}

// run: test_param_out_array_multiple() ~= 6.0

void increment_all(inout float[3] arr) {
    arr[0] = arr[0] + 1.0;
    arr[1] = arr[1] + 1.0;
    arr[2] = arr[2] + 1.0;
}

float test_param_inout_array_all() {
    // Inout parameter with array, modifying all elements
    float[3] arr;
    arr[0] = 1.0;
    arr[1] = 2.0;
    arr[2] = 3.0;
    increment_all(arr);
    return arr[0] + arr[1] + arr[2];
}

// run: test_param_inout_array_all() ~= 9.0
