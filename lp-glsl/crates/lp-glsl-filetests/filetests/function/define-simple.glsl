// test run
// target riscv32.q32

// ============================================================================
// Simple Function Definitions: Basic function implementations
// ============================================================================

float helper_simple(float x) {
    return x * 2.0;
}

float test_define_simple_float() {
    // Simple function definition
    return helper_simple(5.0);
}

// run: test_define_simple_float() ~= 10.0

void helper_simple_void() {
    // Empty function body
}

void test_define_simple_void() {
    // Void function definition
    helper_simple_void();
}

// run: test_define_simple_void() == 0.0

int add_ints_simple(int a, int b) {
    return a + b;
}

int test_define_simple_int() {
    // Integer function definition
    return add_ints_simple(3, 7);
}

// run: test_define_simple_int() == 10

bool and_bool_simple(bool a, bool b) {
    return a && b;
}

bool test_define_simple_bool() {
    // Boolean function definition
    return and_bool_simple(true, false);
}

// run: test_define_simple_bool() == false

float get_pi_simple() {
    return 3.14159;
}

float test_define_simple_no_params() {
    // Function with no parameters
    return get_pi_simple();
}

// run: test_define_simple_no_params() ~= 3.14159

float complex_calc_simple(float x) {
    float temp = x * 2.0;
    temp = temp + 1.0;
    return temp / 2.0;
}

float test_define_simple_multiple_stmts() {
    // Function with multiple statements
    return complex_calc_simple(4.0);
}

// run: test_define_simple_multiple_stmts() ~= 4.5

float inner_func_simple(float y) {
    return y + 1.0;
}

float outer_func_simple(float x) {
    return inner_func_simple(x) * 2.0;
}

float test_define_simple_nested() {
    // Nested function definitions
    return outer_func_simple(3.0);
}

// run: test_define_simple_nested() ~= 8.0
