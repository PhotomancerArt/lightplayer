// test run
// target riscv32.q32

// ============================================================================
// Void Return Type: Functions that return nothing
// ============================================================================

void do_nothing() {
}

void test_return_void_empty() {
    // Void function with empty body
    // do_nothing();
}

// run: test_return_void_empty() == 0.0

void explicit_return() {
    return;
}

void test_return_void_explicit() {
    // Void function with explicit return
    explicit_return();
}

// run: test_return_void_explicit() == 0.0

void process_data() {
    float x = 1.0;
    x = x + 1.0;
    // No return statement needed for void functions
}

void test_return_void_with_statements() {
    // Void function with statements but no return value
    process_data();
}

// run: test_return_void_with_statements() == 0.0

void func_a() {
    // Function A
}

void func_b() {
    // Function B
}

void test_return_void_multiple_calls() {
    // Multiple calls to void functions
    func_a();
    func_b();
    func_a();
}

// run: test_return_void_multiple_calls() == 0.0

void step1() {
    // Step 1
}

void step2() {
    // Step 2
}

void step3() {
    // Step 3
}

void test_return_void_in_sequence() {
    // Void functions called in sequence
    step1();
    step2();
    step3();
}

// run: test_return_void_in_sequence() == 0.0

void consume_value(float value) {
    // Consume the value (do nothing with it)
}

void test_return_void_with_params() {
    // Void function with parameters
    consume_value(42.0);
    consume_value(3.14);
}

// run: test_return_void_with_params() == 0.0

void inner() {
    // Inner void function
}

void outer() {
    inner();
    inner();
}

void test_return_void_nested_calls() {
    // Nested calls to void functions
    outer();
}

// run: test_return_void_nested_calls() == 0.0
