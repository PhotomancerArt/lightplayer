// test run
// target riscv32.q32

// ============================================================================
// Not Equal: float != float -> bool
// ============================================================================

bool test_float_not_equal_different_values() {
    // Inequality with different values
    return 5.0 != 6.0;
}

// run: test_float_not_equal_different_values() == true

bool test_float_not_equal_same_values() {
    return 5.0 != 5.0;
}

// run: test_float_not_equal_same_values() == false

bool test_float_not_equal_negative_different() {
    return (-3.14) != (-3.15);
}

// run: test_float_not_equal_negative_different() == true

bool test_float_not_equal_negative_same() {
    return (-3.14) != (-3.14);
}

// run: test_float_not_equal_negative_same() == false

bool test_float_not_equal_from_zero() {
    return 0.0 != 1.0;
}

// run: test_float_not_equal_from_zero() == true

bool test_float_not_equal_variables_different() {
    float a = 10.5;
    float b = 10.6;
    return a != b;
}

// run: test_float_not_equal_variables_different() == true

bool test_float_not_equal_variables_same() {
    float a = 10.5;
    float b = 10.5;
    return a != b;
}

// run: test_float_not_equal_variables_same() == false

bool test_float_not_equal_expressions() {
    return (2.0 + 3.0) != (1.0 + 3.0);
}

// run: test_float_not_equal_expressions() == true

bool test_float_not_equal_self() {
    float a = 7.5;
    return a != a;
}

// run: test_float_not_equal_self() == false

bool test_float_not_equal_after_assignment() {
    float a = 5.0;
    float b = 3.0;
    b = a;
    return a != b;
}

// run: test_float_not_equal_after_assignment() == false
