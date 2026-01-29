// test run
// target riscv32.q32

// ============================================================================
// Void Functions Cannot Return Values: void functions have no return value
// ============================================================================

void simple_void() {
    float x = 1.0;
    // No return statement needed
}

void test_edge_void_return_value_none() {
    // Void functions don't need to return anything
    simple_void();
}

// run: test_edge_void_return_value_none() == 0.0

void explicit_void_return() {
    float x = 1.0;
    if (x > 0.0) {
        return; // OK: return nothing
    }
    x = x + 1.0;
}

void test_edge_void_return_explicit() {
    // Void functions can have explicit return with no value
    explicit_void_return();
}

// run: test_edge_void_return_explicit() == 0.0

/*
void test_edge_void_return_value_error() {
    // Void functions cannot return values - compile error
    void bad_void() {
        // return 42.0; // ERROR: void function cannot return value
        // return 42; // ERROR: void function cannot return value
        // return vec2(1.0, 2.0); // ERROR: void function cannot return value
    }

    bad_void();
}

// run: test_edge_void_return_value_error() == 0.0 [expect-fail]
*/

void conditional_returns() {
    float value = 5.0;

    if (value < 0.0) {
        return; // Early exit 1
    }

    if (value > 100.0) {
        return; // Early exit 2
    }

    if (value == 0.0) {
        return; // Early exit 3
    }

    // Continue processing
    value = value * 2.0;
}

void test_edge_void_multiple_returns() {
    // Void functions can have multiple return statements (without values)
    conditional_returns();
}

// run: test_edge_void_multiple_returns() == 0.0

/*
void test_edge_void_in_expression() {
    // Void function calls cannot be used in expressions - would be error
    void get_void() {
        // Returns nothing
    }

    // float x = get_void(); // ERROR: cannot assign void to float
    // float y = get_void() + 1.0; // ERROR: cannot use void in expression
    // if (get_void()) { } // ERROR: void cannot be used as condition

    // Valid: call void function as statement
    get_void();
}

// run: test_edge_void_in_expression() == 0.0 [expect-fail]
*/

float helper() {
    return 42.0;
}

void caller() {
    float value = helper(); // OK: call non-void function
    // Do something with value
}

void test_edge_void_calls_other_functions() {
    // Void functions can call other functions
    caller();
}

// run: test_edge_void_calls_other_functions() == 0.0

void consume_value(float x, int y) {
    // Use parameters but return nothing
    float result = x * float(y);
}

void test_edge_void_with_parameters() {
    // Void functions can take parameters
    consume_value(3.14, 2);
}

// run: test_edge_void_with_parameters() == 0.0

void empty() {
}

void test_edge_void_empty_body() {
    // Void functions can have completely empty bodies
    empty();
}

// run: test_edge_void_empty_body() == 0.0

void inner() {
    // Inner void function
}

void outer() {
    inner(); // Call void function
    inner(); // Call again
}

void test_edge_void_nested_calls() {
    // Void functions can call other void functions
    outer();
}

// run: test_edge_void_nested_calls() == 0.0

/*
void test_edge_void_wrong_return_type() {
    // Functions declared as void cannot return values - compile error
    void wrong_return() {
        // return 5.0; // ERROR: void function
        // return; // OK: return nothing
    }

    wrong_return();
}

// run: test_edge_void_wrong_return_type() == 0.0 [expect-fail]
*/

void process_data(float x, float y) {
    if (x < 0.0 || y < 0.0) {
        return; // Early exit for invalid data
    }

    if (x > 100.0 && y > 100.0) {
        return; // Early exit for large values
    }

    // Process valid data
    float result = x + y;
}

void test_edge_void_early_return_complex() {
    // Complex early returns in void functions
    process_data(10.0, 20.0);
}

// run: test_edge_void_early_return_complex() == 0.0
