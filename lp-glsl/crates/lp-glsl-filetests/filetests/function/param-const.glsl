// test run
// target riscv32.q32

// ============================================================================
// Const Parameters: Read-only parameters
// ============================================================================

float use_const(const float value) {
    // value = value + 1.0;  // This would be a compile error
    return value * 2.0;
}

float test_param_const_simple() {
    // Const parameter - cannot be modified
    return use_const(5.0);
}

// run: test_param_const_simple() ~= 10.0

float sum_components(const vec2 v) {
    return v.x + v.y;
}

float test_param_const_in_vector() {
    // Const parameter with vector type
    return sum_components(vec2(3.0, 4.0));
}

// run: test_param_const_in_vector() ~= 7.0

int multiply_by_three(const int value) {
    return value * 3;
}

int test_param_const_int() {
    // Const parameter with integer type
    return multiply_by_three(7);
}

// run: test_param_const_int() == 21

bool negate(const bool flag) {
    return !flag;
}

bool test_param_const_bool() {
    // Const parameter with boolean type
    return negate(true);
}

// run: test_param_const_bool() == false

float complex_calc(const float x, const float y) {
    return (x + y) * (x - y);
}

float test_param_const_in_expression() {
    // Const parameter used in complex expression
    return complex_calc(3.0, 4.0);
}

// run: test_param_const_in_expression() ~= -7.0

vec3 normalize_components(const vec3 v) {
    return v / length(v);
}

vec3 test_param_const_vector_ops() {
    // Const parameter with vector operations
    vec3 result = normalize_components(vec3(3.0, 4.0, 5.0));
    return result;
}

// run: test_param_const_vector_ops() ~= vec3(0.424264, 0.565685, 0.707107)

float helper_const(const float x) {
    return x + 1.0;
}

float process_const(const float value) {
    return helper_const(value) * 2.0;
}

float test_param_const_pass_through() {
    // Const parameter passed to another function
    return process_const(3.0);
}

// run: test_param_const_pass_through() ~= 8.0
