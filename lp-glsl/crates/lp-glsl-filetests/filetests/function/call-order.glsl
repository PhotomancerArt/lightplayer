// test run
// target riscv32.q32

// ============================================================================
// Argument Evaluation Order: Left to right, exactly once
// ============================================================================

float global_counter = 0.0;

float get_next() {
    global_counter = global_counter + 1.0;
    return global_counter;
}

void reset_counter() {
    global_counter = 0.0;
}

float add_three(float a, float b, float c) {
    return a + b + c;
}

float test_call_order_left_to_right() {
    // Arguments evaluated left to right
    reset_counter();
    // Arguments should be evaluated left to right: get_next()=1, get_next()=2, get_next()=3
    // Result should be 1 + 2 + 3 = 6
    return add_three(get_next(), get_next(), get_next());
}

// run: test_call_order_left_to_right() ~= 6.0 [expect-fail]

float increment_counter() {
    global_counter = global_counter + 1.0;
    return global_counter;
}

float sum(float a, float b) {
    return a + b;
}

float test_call_order_exactly_once() {
    // Each argument evaluated exactly once
    global_counter = 0.0;
    // Each call to increment_counter() should happen exactly once
    float result = sum(increment_counter(), increment_counter());
    return result; // Should be 1 + 2 = 3
}

// run: test_call_order_exactly_once() ~= 3.0 [expect-fail]

float side_effect_func(float x) {
    global_counter = global_counter + x;
    return x * 2.0;
}

float multiply(float a, float b) {
    return a * b;
}

float test_call_order_side_effects() {
    // Side effects in argument evaluation
    global_counter = 0.0;
    // side_effect_func(2.0) -> counter += 2, returns 4
    // side_effect_func(3.0) -> counter += 3, returns 6
    // multiply(4, 6) -> returns 24
    // counter should be 5
    float result = multiply(side_effect_func(2.0), side_effect_func(3.0));
    return result + global_counter; // 24 + 5 = 29
}

// run: test_call_order_side_effects() ~= 29.0 [expect-fail]

vec2 make_vec2(float x, float y) {
    global_counter = global_counter + 1.0;
    return vec2(x + global_counter, y + global_counter);
}

vec2 add_vectors(vec2 a, vec2 b) {
    return a + b;
}

vec2 test_call_order_vector_args() {
    // Argument evaluation with vector arguments
    global_counter = 0.0;
    // make_vec2(1,2) -> counter=1, returns vec2(2,3)
    // make_vec2(4,5) -> counter=2, returns vec2(6,7)
    // add_vectors -> vec2(8,10)
    return add_vectors(make_vec2(1.0, 2.0), make_vec2(4.0, 5.0));
}

// run: test_call_order_vector_args() ~= vec2(8.0, 10.0) [expect-fail]

float complex_arg(float base) {
    global_counter = global_counter + 0.5;
    return base + global_counter;
}

float process(float a, float b, float c) {
    return a * b + c;
}

float test_call_order_complex_expression() {
    // Complex expressions as arguments
    global_counter = 0.0;
    // complex_arg(1) -> counter=0.5, returns 1.5
    // complex_arg(2) -> counter=1.0, returns 3.0
    // complex_arg(3) -> counter=1.5, returns 4.5
    // process(1.5, 3.0, 4.5) -> 1.5*3.0 + 4.5 = 9.0
    return process(complex_arg(1.0), complex_arg(2.0), complex_arg(3.0));
}

// run: test_call_order_complex_expression() ~= 9.0 [expect-fail]

float record_value(float val) {
    global_counter = val;
    return val;
}

int record_int(int val) {
    global_counter = float(val) + 10.0;
    return val;
}

float combine(float f, int i) {
    return f + float(i);
}

float test_call_order_mixed_types() {
    // Mixed argument types
    global_counter = 0.0;
    // record_value(3.14) -> counter=3.14, returns 3.14
    // record_int(42) -> counter=52.0, returns 42
    // combine(3.14, 42) -> 45.14
    return combine(record_value(3.14), record_int(42));
}

// run: test_call_order_mixed_types() ~= 45.14 [expect-fail]

