// test run
// target riscv32.q32

// ============================================================================
// Static Recursion Detection: Should produce compile errors
// Note: These tests are expected to FAIL compilation due to recursion
// ============================================================================

/*
// Direct recursion - should be compile error
float factorial(int n) {
    if (n <= 1) return 1.0;
    return float(n) * factorial(n - 1); // Direct recursion - ERROR
}

float test_recursive_direct() {
    return factorial(5);
}

// run: test_recursive_direct() ~= 120.0 [expect-fail]
*/

/*
// Indirect recursion - should be compile error
float func_a(int x) {
    if (x <= 0) return 0.0;
    return func_b(x - 1) + 1.0; // Calls func_b
}

float func_b(int x) {
    return func_a(x); // Calls func_a - indirect recursion - ERROR
}

float test_recursive_indirect() {
    return func_a(3);
}

// run: test_recursive_indirect() ~= 3.0 [expect-fail]
*/

/*
// Mutual recursion - should be compile error
float even(int n) {
    if (n == 0) return 1.0;
    return odd(n - 1); // Calls odd
}

float odd(int n) {
    if (n == 0) return 0.0;
    return even(n - 1); // Calls even - mutual recursion - ERROR
}

float test_recursive_mutual() {
    return even(4);
}

// run: test_recursive_mutual() ~= 1.0 [expect-fail]
*/

float fibonacci_iterative(int n) {
    if (n <= 1) return float(n);
    float a = 0.0, b = 1.0, temp;
    for (int i = 2; i <= n; i++) {
        temp = a + b;
        a = b;
        b = temp;
    }
    return b;
}

float test_recursive_allowed() {
    // Non-recursive functions - should work fine
    return fibonacci_iterative(6); // Should be 8.0
}

// run: test_recursive_allowed() ~= 8.0

/*
// Deep recursion chain - should still be detected as error
float level1(int x) {
    if (x <= 0) return 1.0;
    return level2(x - 1);
}

float level2(int x) {
    return level3(x);
}

float level3(int x) {
    return level1(x); // Creates cycle: 1->2->3->1 - ERROR
}

float test_recursive_deep() {
    return level1(2);
}

// run: test_recursive_deep() ~= 1.0 [expect-fail]
*/

float double_it(float x) {
    return x * 2.0;
}

float add_then_double(float a, float b) {
    return double_it(a + b);
}

float square_then_add(float x) {
    return add_then_double(x, x * x);
}

float test_functions_calling_each_other() {
    // Different functions calling each other without cycles - OK
    return square_then_add(2.0); // ((2.0) + (2.0 * 2.0)) * 2.0 = 12.0
}

// run: test_functions_calling_each_other() ~= 12.0
