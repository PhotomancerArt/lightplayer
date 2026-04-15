// test run

// ============================================================================
// Cross-function: Globals shared across call tree within one invocation
// ============================================================================

float shared_state = 0.0;

void helper_increment() {
    shared_state += 10.0;
}

float test_cross_function_helper_mutates() {
    shared_state = 1.0;
    helper_increment();
    return shared_state;
}

// Helper sets shared_state to 1.0 + 10.0 = 11.0
// run: test_cross_function_helper_mutates() ~= 11.0

void helper_set(float val) {
    shared_state = val;
}

float helper_get() {
    return shared_state;
}

float test_cross_function_set_get() {
    helper_set(42.0);
    return helper_get();
}

// run: test_cross_function_set_get() ~= 42.0

float accumulator = 0.0;

void add_to_accumulator(float x) {
    accumulator += x;
}

float test_cross_function_accumulate() {
    add_to_accumulator(1.0);
    add_to_accumulator(2.0);
    add_to_accumulator(3.0);
    return accumulator;
}

// run: test_cross_function_accumulate() ~= 6.0
