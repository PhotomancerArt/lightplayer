// test run
// target riscv32.q32

// ============================================================================
// Nested Function Calls: Functions calling other functions
// ============================================================================

float double_nested(float x) {
    return x * 2.0;
}

float add_one_nested(float x) {
    return x + 1.0;
}

float test_call_nested_simple() {
    // Simple nested calls
    return double_nested(add_one_nested(3.0)); // double(add_one(3)) = double(4) = 8
}

// run: test_call_nested_simple() ~= 8.0

float func_a_nested(float x) {
    return x + 1.0;
}

float func_b_nested(float x) {
    return func_a_nested(x) * 2.0;
}

float func_c_nested(float x) {
    return func_b_nested(x) + 3.0;
}

float test_call_nested_deep() {
    // Deep nesting
    return func_c_nested(2.0); // func_b(func_a(2)) + 3 = func_b(3)*2 + 3 = 6 + 3 = 9
}

// run: test_call_nested_deep() ~= 9.0

vec2 scale_nested(vec2 v, float s) {
    return v * s;
}

vec2 rotate_nested(vec2 v) {
    return vec2(-v.y, v.x);
}

vec2 transform_nested(vec2 v) {
    return scale_nested(rotate_nested(v), 2.0);
}

vec2 test_call_nested_vector() {
    // Nested calls with vectors
    vec2 input = vec2(1.0, 0.0);
    return transform_nested(input); // scale(rotate(vec2(1,0)), 2) = scale(vec2(0,1), 2) = vec2(0,2)
}

// run: test_call_nested_vector() ~= vec2(0.0, 2.0)

float get_length_nested(vec2 v) {
    return length(v);
}

int round_to_int_nested(float x) {
    return int(x + 0.5);
}

float process_vector_nested(vec2 v) {
    return float(round_to_int_nested(get_length_nested(v)));
}

float test_call_nested_mixed_types() {
    // Nested calls with different return types
    return process_vector_nested(vec2(3.0, 4.0)); // round_to_int(length(vec2(3,4))) = round_to_int(5) = 5
}

// run: test_call_nested_mixed_types() ~= 5.0

float square_nested(float x) {
    return x * x;
}

float cube_nested(float x) {
    return x * x * x;
}

float complex_calc_nested(float a, float b) {
    return square_nested(a) + cube_nested(b);
}

float test_call_nested_multiple_paths() {
    // Multiple nested call paths
    return complex_calc_nested(2.0, 3.0); // square(2) + cube(3) = 4 + 27 = 31
}

// run: test_call_nested_multiple_paths() ~= 31.0

mat2 identity_nested() {
    return mat2(1.0);
}

mat2 scale_matrix_nested(float s) {
    return mat2(s, 0.0, 0.0, s);
}

mat2 combine_transforms_nested(mat2 a, mat2 b) {
    return a * b;
}

mat2 test_call_nested_matrix() {
    // Nested calls with matrices
    return combine_transforms_nested(scale_matrix_nested(2.0), identity_nested());
}

// run: test_call_nested_matrix() ~= mat2(2.0, 0.0, 0.0, 2.0)

float apply_twice_nested(float x, float factor) {
    return x * factor * factor;
}

float process_nested(float x) {
    return apply_twice_nested(x, 3.0);
}

float test_call_nested_recursive_pattern() {
    // Non-recursive nested pattern
    return process_nested(2.0); // apply_twice(2, 3) = 2 * 3 * 3 = 18
}

// run: test_call_nested_recursive_pattern() ~= 18.0

float increment_nested(float x) {
    return x + 1.0;
}

float test_call_nested_chained() {
    // Chained nested calls
    // Chain of increments: ((2+1)+1)+1 = 5
    return increment_nested(increment_nested(increment_nested(2.0)));
}

// run: test_call_nested_chained() ~= 5.0

