// test run
// target riscv32.q32

// ============================================================================
// Edge cases and special scenarios
// Spec: Various edge cases for ternary operator
// ============================================================================

// Using ternary in return statement
int test_ternary_return_direct() {
    bool b = true;
    return b ? 42 : 24;
}

// run: test_ternary_return_direct() == 42

// Using ternary in variable initialization
int test_ternary_initialization() {
    bool b = false;
    int x = b ? 10 : 20;
    return x;
}

// run: test_ternary_initialization() == 20

// Using ternary with function calls
int helper_func1() {
    return 100;
}

int helper_func2() {
    return 200;
}

int test_ternary_with_function_calls() {
    bool b = true;
    return b ? helper_func1() : helper_func2();
}

// run: test_ternary_with_function_calls() == 100

// Using ternary in arithmetic operations
int test_ternary_in_addition() {
    bool b = false;
    return 5 + (b ? 10 : 20);
}

// run: test_ternary_in_addition() == 25

int test_ternary_in_multiplication() {
    bool b = true;
    return 3 * (b ? 4 : 5);
}

// run: test_ternary_in_multiplication() == 12

// Using ternary with comparisons
int test_ternary_in_comparison() {
    bool b = true;
    return (b ? 10 : 20) > 15 ? 1 : 0;
}

// run: test_ternary_in_comparison() == 0

int test_ternary_comparison_result() {
    bool b = false;
    return (b ? 10 : 20) == 20 ? 1 : 0;
}

// run: test_ternary_comparison_result() == 1

// Using ternary with logical operators
int test_ternary_with_logical_and() {
    bool a = true;
    bool b = false;
    return (a && b) ? 10 : 20;
}

// run: test_ternary_with_logical_and() == 20

int test_ternary_with_logical_or() {
    bool a = false;
    bool b = true;
    return (a || b) ? 10 : 20;
}

// run: test_ternary_with_logical_or() == 10

int test_ternary_with_logical_not() {
    bool a = false;
    return (!a) ? 10 : 20;
}

// run: test_ternary_with_logical_not() == 10

// Using ternary with complex expressions
int test_ternary_complex_condition() {
    int x = 5;
    int y = 3;
    int z = 7;
    return (x > y && z > x) ? 100 : 200;
}

// run: test_ternary_complex_condition() == 100

int test_ternary_complex_branches() {
    bool b = true;
    int x = 5;
    int y = 3;
    return b ? (x + y) : (x - y);
}

// run: test_ternary_complex_branches() == 8

// Using ternary with constants
int test_ternary_constant_true() {
    return true ? 1 : 0;
}

// run: test_ternary_constant_true() == 1

int test_ternary_constant_false() {
    return false ? 1 : 0;
}

// run: test_ternary_constant_false() == 0

// Using ternary with same values (should still work)
int test_ternary_same_values() {
    bool b = true;
    return b ? 42 : 42;
}

// run: test_ternary_same_values() == 42

// Using ternary with zero
int test_ternary_with_zero() {
    bool b = false;
    return b ? 10 : 0;
}

// run: test_ternary_with_zero() == 0

// Using ternary with negative values
int test_ternary_with_negative() {
    bool b = true;
    return b ? -10 : -20;
}

// run: test_ternary_with_negative() == -10

// Using ternary in conditional assignment
int test_ternary_conditional_assignment() {
    int x = 5;
    bool b = true;
    x = b ? 10 : x;
    return x;
}

// run: test_ternary_conditional_assignment() == 10

// Using ternary with increment
int test_ternary_with_increment() {
    int x = 5;
    bool b = false;
    x = b ? x + 1 : x + 2;
    return x;
}

// run: test_ternary_with_increment() == 7

// Multiple ternaries in one expression
int test_ternary_multiple_in_expression() {
    bool a = true;
    bool b = false;
    return (a ? 10 : 20) + (b ? 5 : 15);
}

// run: test_ternary_multiple_in_expression() == 25

// Ternary as part of larger expression
int test_ternary_in_large_expression() {
    bool b = true;
    return 2 * (b ? 3 : 4) + 5;
}

// run: test_ternary_in_large_expression() == 11





