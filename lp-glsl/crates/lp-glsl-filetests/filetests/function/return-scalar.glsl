// test run
// target riscv32.q32

// ============================================================================
// Scalar Return Types: float, int, uint, bool
// ============================================================================

// Helper functions (top-level)
float get_pi() {
    return 3.14159;
}

float test_return_float_simple() {
    // Return float value
    return get_pi();
}

// run: test_return_float_simple() ~= 3.14159

int get_answer() {
    return 42;
}

int test_return_int_simple() {
    // Return int value
    return get_answer();
}

// run: test_return_int_simple() == 42

uint get_count() {
    return 100u;
}

uint test_return_uint_simple() {
    // Return uint value
    return get_count();
}

// run: test_return_uint_simple() == 100u

bool get_truth() {
    return true;
}

bool test_return_bool_simple() {
    // Return bool value
    return get_truth();
}

// run: test_return_bool_simple() == true

float calculate_area(float radius) {
    return 3.14159 * radius * radius;
}

float test_return_float_calculation() {
    // Return result of calculation
    return calculate_area(2.0);
}

// run: test_return_float_calculation() ~= 12.56636

int add_numbers(int a, int b, int c) {
    return a + b + c;
}

int test_return_int_arithmetic() {
    // Return result of integer arithmetic
    return add_numbers(1, 2, 3);
}

// run: test_return_int_arithmetic() == 6

bool is_even(int x) {
    return (x % 2) == 0;
}

bool test_return_bool_logic() {
    // Return result of boolean logic
    return is_even(4) && !is_even(3);
}

// run: test_return_bool_logic() == true

float int_to_float(int x) {
    return float(x);
}

float test_return_float_conversion() {
    // Return float converted from int
    return int_to_float(5);
}

// run: test_return_float_conversion() ~= 5.0

int bool_to_int(bool b) {
    return b ? 1 : 0;
}

int test_return_int_from_bool() {
    // Return int converted from bool
    return bool_to_int(true) + bool_to_int(false);
}

// run: test_return_int_from_bool() == 1
