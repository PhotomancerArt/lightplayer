// test run
// target riscv32.q32

// ============================================================================
// Function Parameters with Const-Sized Arrays
// ============================================================================

const int PARAM_SIZE = 5;

// Function parameter with const-sized array
float test_param_array(float arr[PARAM_SIZE]) {
    return arr[0];
}

// Multiple parameters with same const size
float test_multiple_params(float a[PARAM_SIZE], int b[PARAM_SIZE]) {
    return a[0] + float(b[0]);
}

// Const expression in parameter
const int PARAM_EXPR = 2 * 3;
void test_param_expr(vec2 arr[PARAM_EXPR]) {
    // Function body
}

// Different const sizes for different parameters
const int SIZE_A = 3;
const int SIZE_B = 4;
float test_different_sizes(vec3 arr_a[SIZE_A], vec4 arr_b[SIZE_B]) {
    return 1.0;
}

// Nested function calls with const-sized parameters
const int NESTED_SIZE = 2;
vec2 helper_func(vec2 arr[NESTED_SIZE]) {
    return arr[0];
}

vec2 test_nested_calls(vec2 arr[NESTED_SIZE]) {
    return helper_func(arr);
}

// Function returning const-sized array
const int RETURN_SIZE = 3;
vec3[RETURN_SIZE] test_return_const_size() {
    vec3[RETURN_SIZE] result;
    return result;
}

// Parameter with complex const expression
const int COMPLEX_PARAM_SIZE = (5 + 3) * 2;
int test_complex_param_expr(float arr[COMPLEX_PARAM_SIZE]) {
    return 1;
}

// Test function calls
float test_param_array_call() {
    float test_arr[PARAM_SIZE];
    return test_param_array(test_arr);
}

// run: test_param_array_call() == 0.0 [expect-fail]

float test_multiple_params_call() {
    float arr_a[PARAM_SIZE];
    int arr_b[PARAM_SIZE];
    return test_multiple_params(arr_a, arr_b);
}

// run: test_multiple_params_call() == 0.0 [expect-fail]

vec2 test_nested_calls_call() {
    vec2 test_arr[NESTED_SIZE];
    return test_nested_calls(test_arr);
}

// run: test_nested_calls_call() ~= vec2(0.0, 0.0) [expect-fail]

vec3 test_return_const_size_call() {
    vec3[RETURN_SIZE] result = test_return_const_size();
    return result[0];
}

// run: test_return_const_size_call() ~= vec3(0.0, 0.0, 0.0) [expect-fail]

int test_complex_param_expr_call() {
    float test_arr[COMPLEX_PARAM_SIZE];
    return test_complex_param_expr(test_arr);
}

// run: test_complex_param_expr_call() == 1 [expect-fail]

float test_different_sizes_call() {
    vec3 arr_a[SIZE_A];
    vec4 arr_b[SIZE_B];
    return test_different_sizes(arr_a, arr_b);
}

// run: test_different_sizes_call() == 1.0 [expect-fail]




