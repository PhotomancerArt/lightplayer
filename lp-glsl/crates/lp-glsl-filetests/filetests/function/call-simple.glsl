// test run
// target riscv32.q32

// ============================================================================
// Simple Function Calls: Basic function invocation
// ============================================================================

float square(float x) {
    return x * x;
}

float test_call_simple_single_arg() {
    // Call function with single argument
    return square(4.0);
}

// run: test_call_simple_single_arg() ~= 16.0

float add(float a, float b) {
    return a + b;
}

float test_call_simple_multiple_args() {
    // Call function with multiple arguments
    return add(3.0, 7.0);
}

// run: test_call_simple_multiple_args() ~= 10.0

void print_value(float x) {
    // In a real shader this would output, but we just call it
}

void test_call_simple_void() {
    // Call void function
    print_value(42.0);
}

// run: test_call_simple_void() == 0.0

float get_constant() {
    return 3.14;
}

float test_call_simple_no_args() {
    // Call function with no arguments
    return get_constant();
}

// run: test_call_simple_no_args() ~= 3.14

float double(float x) {
    return x * 2.0;
}

float test_call_simple_in_expression() {
    // Function call in expression
    return double(5.0) + double(3.0);
}

// run: test_call_simple_in_expression() ~= 16.0

float add_one(float x) {
    return x + 1.0;
}

float multiply_by_two(float x) {
    return x * 2.0;
}

float test_call_simple_nested_calls() {
    // Nested function calls
    return multiply_by_two(add_one(3.0));
}

// run: test_call_simple_nested_calls() ~= 8.0

float get_value() {
    return 42.0;
}

float test_call_simple_in_assignment() {
    // Function call in assignment
    float result = get_value();
    return result;
}

// run: test_call_simple_in_assignment() ~= 42.0

bool is_positive(float x) {
    return x > 0.0;
}

bool test_call_simple_in_condition() {
    // Function call in conditional expression
    return is_positive(5.0) && is_positive(-3.0);
}

// run: test_call_simple_in_condition() == false

