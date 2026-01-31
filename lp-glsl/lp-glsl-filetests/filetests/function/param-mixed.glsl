// test run
// target riscv32.q32

// ============================================================================
// Mixed Parameter Qualifiers: in, out, inout combinations
// ============================================================================

void process_value(in float input, out float output) {
    output = input * 2.0;
}

void test_param_mixed_in_out() {
    // Mix of in and out parameters
    float result;
    process_value(5.0, result);
    // Result should be 10.0
}

// run: test_param_mixed_in_out() == 0.0

void modify_value(in float multiplier, inout float value) {
    value = value * multiplier;
}

void test_param_mixed_in_inout() {
    // Mix of in and inout parameters
    float x = 3.0;
    modify_value(4.0, x);
    // x should be 12.0
}

// run: test_param_mixed_in_inout() == 0.0

void swap_and_double(out float result, inout float value) {
    result = value * 2.0;
    value = value + 1.0;
}

void test_param_mixed_out_inout() {
    // Mix of out and inout parameters
    float out_val, inout_val = 5.0;
    swap_and_double(out_val, inout_val);
    // out_val should be 10.0, inout_val should be 6.0
}

// run: test_param_mixed_out_inout() == 0.0

void complex_op(in float a, out float b, inout float c) {
    b = a + c;
    c = a * c;
}

void test_param_mixed_all_three() {
    // All three parameter qualifiers
    float out_val, inout_val = 3.0;
    complex_op(2.0, out_val, inout_val);
    // out_val should be 5.0, inout_val should be 6.0
}

// run: test_param_mixed_all_three() == 0.0

void vector_ops(in vec2 input, out vec2 doubled, inout vec2 scaled) {
    doubled = input * 2.0;
    scaled = scaled * 3.0 + input;
}

vec2 test_param_mixed_vector() {
    // Mixed qualifiers with vector types
    vec2 out_vec, inout_vec = vec2(1.0, 2.0);
    vector_ops(vec2(1.0, 1.0), out_vec, inout_vec);
    return out_vec + inout_vec;
}

// run: test_param_mixed_vector() ~= vec2(6.0, 9.0)

void int_ops(in int base, out int doubled, inout int incremented) {
    doubled = base * 2;
    incremented = incremented + base;
}

int test_param_mixed_int() {
    // Mixed qualifiers with integer types
    int out_val, inout_val = 10;
    int_ops(5, out_val, inout_val);
    return out_val + inout_val;
}

// run: test_param_mixed_int() == 25

void ordered_ops(in float a, in float b, out float sum, out float product) {
    sum = a + b;
    product = a * b;
}

float test_param_mixed_order() {
    // Test parameter evaluation order (left to right)
    float sum_result, product_result;
    ordered_ops(3.0, 4.0, sum_result, product_result);
    return sum_result + product_result;
}

// run: test_param_mixed_order() ~= 19.0
