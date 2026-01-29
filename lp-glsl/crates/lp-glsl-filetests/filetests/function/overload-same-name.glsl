// test run
// target riscv32.q32

// ============================================================================
// Function Overloading: Same name, different parameter types
// ============================================================================

// Overloaded functions with different parameter types (top-level)
float add_float(float a, float b) {
    return a + b;
}

int add_int(int a, int b) {
    return a + b;
}

uint add_uint(uint a, uint b) {
    return a + b;
}

float test_overload_same_name() {
    // Overloaded add functions
    // Test calling different overloads
    float result = add_float(1.5, 2.5) + float(add_int(3, 4)) + float(add_uint(5u, 6u));
    return result;
}

// run: test_overload_same_name() ~= 21.0 [expect-fail]

float length_squared_vec2(vec2 v) {
    return dot(v, v);
}

float length_squared_vec3(vec3 v) {
    return dot(v, v);
}

float length_squared_vec4(vec4 v) {
    return dot(v, v);
}

float test_overload_vector_types() {
    // Overloaded functions with different vector types
    // Test different vector overloads
    float result = length_squared_vec2(vec2(3.0, 4.0)) +  // 25.0
                   length_squared_vec3(vec3(1.0, 2.0, 2.0)) +  // 9.0
                   length_squared_vec4(vec4(1.0, 1.0, 1.0, 1.0)); // 4.0
    return result;
}

// run: test_overload_vector_types() ~= 38.0 [expect-fail]

vec2 scale_vec2(vec2 v, float s) {
    return v * s;
}

vec3 scale_vec3(vec3 v, float s) {
    return v * s;
}

vec4 scale_vec4(vec4 v, float s) {
    return v * s;
}

float test_overload_mixed_types() {
    // Overloaded functions mixing scalar and vector types
    // Test scaling different vector types
    float result = scale_vec2(vec2(1.0, 2.0), 2.0).x +  // 2.0
                   scale_vec3(vec3(1.0, 2.0, 3.0), 2.0).y +  // 4.0
                   scale_vec4(vec4(1.0, 2.0, 3.0, 4.0), 2.0).z; // 6.0
    return result;
}

// run: test_overload_mixed_types() ~= 12.0 [expect-fail]

float sum_1(float a) {
    return a;
}

float sum_2(float a, float b) {
    return a + b;
}

float sum_3(float a, float b, float c) {
    return a + b + c;
}

float test_overload_parameter_count() {
    // Overloaded functions with different parameter counts
    // Test different parameter counts
    float result = sum_1(1.0) + sum_2(1.0, 2.0) + sum_3(1.0, 2.0, 3.0);
    return result;
}

// run: test_overload_parameter_count() ~= 12.0 [expect-fail]

float determinant2(mat2 m) {
    return m[0][0] * m[1][1] - m[0][1] * m[1][0];
}

float determinant3(mat3 m) {
    return determinant(m);
}

float test_overload_matrix_types() {
    // Overloaded functions with different matrix types
    // Test matrix determinant overloads
    mat2 m2 = mat2(1.0, 2.0, 3.0, 4.0);
    mat3 m3 = mat3(1.0);
    float result = determinant2(m2) + determinant3(m3);
    return result;
}

// run: test_overload_matrix_types() ~= -1.0 [expect-fail]

bool is_zero_float(float x) {
    return x == 0.0;
}

bool is_zero_int(int x) {
    return x == 0;
}

bool is_zero_vec2(vec2 v) {
    return all(equal(v, vec2(0.0)));
}

bool test_overload_bool_types() {
    // Overloaded functions with boolean types
    // Test boolean overloads
    return is_zero_float(0.0) && is_zero_int(0) && is_zero_vec2(vec2(0.0, 0.0));
}

// run: test_overload_bool_types() == true [expect-fail]

float sum_array_arr2(float[2] arr) {
    return arr[0] + arr[1];
}

float sum_array_arr3(float[3] arr) {
    return arr[0] + arr[1] + arr[2];
}

float test_overload_array_types() {
    // Overloaded functions with array types
    // Test array overloads
    float[2] arr2 = float[2](1.0, 2.0);
    float[3] arr3 = float[3](1.0, 2.0, 3.0);
    return sum_array_arr2(arr2) + sum_array_arr3(arr3);
}

// run: test_overload_array_types() ~= 9.0 [expect-fail]
