// test run
// target riscv32.q32

// ============================================================================
// Return with expression
// ============================================================================

int test_return_with_value() {
    return 42;
}

// run: test_return_with_value() == 42

int test_return_with_expression() {
    int x = 10;
    int y = 20;
    return x + y;
}

// run: test_return_with_expression() == 30

int test_return_with_computation() {
    int a = 5;
    int b = 3;
    return a * b + 2;
}

// run: test_return_with_computation() == 17

float test_return_float() {
    return 3.14;
}

// run: test_return_float() ~= 3.14

float test_return_float_expression() {
    float x = 1.5;
    float y = 2.5;
    return x + y;
}

// run: test_return_float_expression() ~= 4.0





