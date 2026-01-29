// test run
// target riscv32.q32

// ============================================================================
// Overload Resolution: Choosing the best matching overload
// ============================================================================

// Overloaded functions (top-level)
float process_float(float x) {
    return x * 2.0;
}

int process_int(int x) {
    return x * 3;
}

float test_overload_resolution_exact_match() {
    // Exact match preferred over conversions
    // Exact matches
    return float(process_float(5.0)) + float(process_int(5)); // Should be 10.0 + 15 = 25.0
}

// run: test_overload_resolution_exact_match() ~= 25.0 [expect-fail]

float accept_float(float x) {
    return x;
}

float accept_int(int x) {
    return float(x) + 0.5;
}

float test_overload_resolution_conversions() {
    // Implicit conversions when exact match not found
    // int to float conversion
    return accept_float(5) + accept_int(3); // 5.0 + (3.0 + 0.5) = 8.5
}

// run: test_overload_resolution_conversions() ~= 8.5 [expect-fail]

float length_func_vec2(vec2 v) {
    return length(v) + 10.0;
}

float length_func_vec3(vec3 v) {
    return length(v) + 20.0;
}

float length_func_vec4(vec4 v) {
    return length(v) + 30.0;
}

float test_overload_resolution_vector_promotion() {
    // Vector type promotions
    // Test vec3 input
    return length_func_vec3(vec3(1.0, 0.0, 0.0)); // Should match vec3 overload
}

// run: test_overload_resolution_vector_promotion() ~= 21.0 [expect-fail]

float mix_func_float(float a, float b) {
    return a + b;
}

float mix_func_int(int a, int b) {
    return float(a + b) + 0.1;
}

float test_overload_resolution_mixed_precision() {
    // Mixed precision handling
    // Mixed types - should find best match
    return mix_func_float(1.0, 2) + mix_func_int(3, 4); // 3.0 + 7.1 = 10.1
}

// run: test_overload_resolution_mixed_precision() ~= 10.1 [expect-fail]

vec2 make_vec2_xy(float x, float y) {
    return vec2(x, y) * 2.0;
}

vec2 make_vec2_v(vec2 v) {
    return v * 3.0;
}

vec2 test_overload_resolution_vector_construction() {
    // Vector construction overloads
    // Should choose vec2(vec2) over vec2(float, float)
    vec2 input = vec2(1.0, 2.0);
    return make_vec2_v(input); // Should be vec2(3.0, 6.0)
}

// run: test_overload_resolution_vector_construction() ~= vec2(3.0, 6.0) [expect-fail]

mat2 transform_mat2(mat2 m) {
    return m * 2.0;
}

mat3 transform_mat3(mat3 m) {
    return m * 3.0;
}

float test_overload_resolution_matrix_ops() {
    // Matrix operation overloads
    // Test mat2 input
    mat2 input = mat2(1.0, 0.0, 0.0, 1.0);
    mat2 result = transform_mat2(input);
    return result[0][0] + result[1][1]; // Should be 4.0
}

// run: test_overload_resolution_matrix_ops() ~= 4.0 [expect-fail]

float sum_elements_arr2(float[2] arr) {
    return arr[0] + arr[1] + 1.0;
}

float sum_elements_arr3(float[3] arr) {
    return arr[0] + arr[1] + arr[2] + 2.0;
}

float test_overload_resolution_array_sizes() {
    // Array size overloads
    // Test different array sizes
    float[2] arr2 = float[2](1.0, 2.0);
    float[3] arr3 = float[3](1.0, 2.0, 3.0);
    return sum_elements_arr2(arr2) + sum_elements_arr3(arr3); // 4.0 + 8.0 = 12.0
}

// run: test_overload_resolution_array_sizes() ~= 12.0 [expect-fail]

bool check_value_bool(bool b) {
    return b;
}

bool check_value_int(int i) {
    return i != 0;
}

bool check_value_float(float f) {
    return f != 0.0;
}

bool test_overload_resolution_bool_conversions() {
    // Boolean conversion overloads
    // Test conversions to bool
    return check_value_bool(true) && check_value_int(1) && check_value_float(1.0);
}

// run: test_overload_resolution_bool_conversions() == true [expect-fail]
