// test run
// target riscv32.q32

// ============================================================================
// Using Return Values: Function calls in expressions and assignments
// ============================================================================

float get_pi() {
    return 3.14159;
}

float test_call_return_value_assignment() {
    // Assign return value to variable
    float pi = get_pi();
    return pi;
}

// run: test_call_return_value_assignment() ~= 3.14159 [expect-fail]

float get_base() {
    return 10.0;
}

float get_multiplier() {
    return 3.0;
}

float test_call_return_value_arithmetic() {
    // Use return value in arithmetic expression
    return get_base() * get_multiplier() + 5.0; // 10 * 3 + 5 = 35
}

// run: test_call_return_value_arithmetic() ~= 35.0 [expect-fail]

float get_value() {
    return 42.0;
}

bool test_call_return_value_comparison() {
    // Use return value in comparison
    return get_value() > 40.0 && get_value() < 50.0;
}

// run: test_call_return_value_comparison() == true [expect-fail]

float get_x() {
    return 1.0;
}

float get_y() {
    return 2.0;
}

vec2 test_call_return_value_vector_expr() {
    // Use return values in vector construction
    vec2 result = vec2(get_x(), get_y()) * 2.0;
    return result;
}

// run: test_call_return_value_vector_expr() ~= vec2(2.0, 4.0) [expect-fail]

float add(float a, float b) {
    return a + b;
}

float multiply(float a, float b) {
    return a * b;
}

float test_call_return_value_nested_expr() {
    // Nested expressions with return values
    return multiply(add(2.0, 3.0), add(4.0, 5.0)); // (2+3) * (4+5) = 5 * 9 = 45
}

// run: test_call_return_value_nested_expr() ~= 45.0 [expect-fail]

float get_increment() {
    return 2.0;
}

float test_call_return_value_in_loop() {
    // Return values used in loop conditions and bodies
    float sum = 0.0;
    for (int i = 0; i < 5; i++) {
        sum = sum + get_increment();
    }
    return sum; // 2+2+2+2+2 = 10
}

// run: test_call_return_value_in_loop() ~= 10.0 [expect-fail]

vec3 get_color() {
    return vec3(1.0, 0.5, 0.0);
}

float test_call_return_value_swizzle() {
    // Return values used with swizzling
    vec2 coords = get_color().xy; // Extract xy components
    return coords.x + coords.y; // 1.0 + 0.5 = 1.5
}

// run: test_call_return_value_swizzle() ~= 1.5 [expect-fail]

bool is_positive(float x) {
    return x > 0.0;
}

float get_negative_value() {
    return -5.0;
}

float test_call_return_value_ternary() {
    // Return values in ternary expressions
    float value = get_negative_value();
    return is_positive(value) ? value : -value; // Should be 5.0 (absolute value)
}

// run: test_call_return_value_ternary() ~= 5.0 [expect-fail]

int get_index() {
    return 1;
}

float test_call_return_value_array_index() {
    // Return values used as array indices
    float[3] arr = float[3](10.0, 20.0, 30.0);
    return arr[get_index()]; // arr[1] = 20.0
}

// run: test_call_return_value_array_index() ~= 20.0 [expect-fail]

float square(float x) {
    return x * x;
}

float get_base_value() {
    return 4.0;
}

float test_call_return_value_function_arg() {
    // Return values as function arguments
    return square(get_base_value()); // square(4) = 16
}

// run: test_call_return_value_function_arg() ~= 16.0 [expect-fail]

