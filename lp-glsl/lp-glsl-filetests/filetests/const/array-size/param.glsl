// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant integral expression"
// Const in function parameter array size.

const int PARAM_SIZE = 5;

float test_param_array(float arr[PARAM_SIZE]) {
    return arr[0];
}

const int NESTED_SIZE = 2;
vec2 helper_func(vec2 arr[NESTED_SIZE]) {
    return arr[0];
}

float test_param_array_call() {
    float test_arr[PARAM_SIZE];
    return test_param_array(test_arr);
}

// run: test_param_array_call() == 0.0

vec2 test_nested_calls(vec2 arr[NESTED_SIZE]) {
    return helper_func(arr);
}

vec2 test_nested_calls_call() {
    vec2 test_arr[NESTED_SIZE];
    return test_nested_calls(test_arr);
}

// run: test_nested_calls_call() ~= vec2(0.0, 0.0)
