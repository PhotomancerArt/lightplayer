// test run
// target riscv32.q32

// ============================================================================
// Equal: int == int -> bool
// ============================================================================

bool test_int_equal_same_values() {
    // Equality with same values
    return 5 == 5;
}

// run: test_int_equal_same_values() == true

bool test_int_equal_different_values() {
    return 5 == 6;
}

// run: test_int_equal_different_values() == false

bool test_int_equal_negative_same() {
    return (-10) == (-10);
}

// run: test_int_equal_negative_same() == true

bool test_int_equal_negative_different() {
    return (-10) == (-11);
}

// run: test_int_equal_negative_different() == false

bool test_int_equal_zero() {
    return 0 == 0;
}

// run: test_int_equal_zero() == true

bool test_int_equal_variables_same() {
    int a = 25;
    int b = 25;
    return a == b;
}

// run: test_int_equal_variables_same() == true

bool test_int_equal_variables_different() {
    int a = 25;
    int b = 26;
    return a == b;
}

// run: test_int_equal_variables_different() == false

bool test_int_equal_expressions() {
    return (5 + 3) == (2 * 4);
}

// run: test_int_equal_expressions() == true

bool test_int_equal_self() {
    int a = 42;
    return a == a;
}

// run: test_int_equal_self() == true

bool test_int_equal_after_assignment() {
    int a = 15;
    int b = 10;
    b = a;
    return a == b;
}

// run: test_int_equal_after_assignment() == true
