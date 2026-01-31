// test run
// target riscv32.q32

// ============================================================================
// Not Equal: int != int -> bool
// ============================================================================

bool test_int_not_equal_different_values() {
    // Inequality with different values
    return 5 != 6;
}

// run: test_int_not_equal_different_values() == true

bool test_int_not_equal_same_values() {
    return 5 != 5;
}

// run: test_int_not_equal_same_values() == false

bool test_int_not_equal_negative_different() {
    return (-10) != (-11);
}

// run: test_int_not_equal_negative_different() == true

bool test_int_not_equal_negative_same() {
    return (-10) != (-10);
}

// run: test_int_not_equal_negative_same() == false

bool test_int_not_equal_from_zero() {
    return 0 != 5;
}

// run: test_int_not_equal_from_zero() == true

bool test_int_not_equal_variables_different() {
    int a = 25;
    int b = 26;
    return a != b;
}

// run: test_int_not_equal_variables_different() == true

bool test_int_not_equal_variables_same() {
    int a = 25;
    int b = 25;
    return a != b;
}

// run: test_int_not_equal_variables_same() == false

bool test_int_not_equal_expressions() {
    return (5 + 3) != (2 * 5);
}

// run: test_int_not_equal_expressions() == true

bool test_int_not_equal_self() {
    int a = 42;
    return a != a;
}

// run: test_int_not_equal_self() == false

bool test_int_not_equal_after_assignment() {
    int a = 15;
    int b = 10;
    b = a;
    return a != b;
}

// run: test_int_not_equal_after_assignment() == false
