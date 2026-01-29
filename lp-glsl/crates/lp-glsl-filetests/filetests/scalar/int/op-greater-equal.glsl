// test run
// target riscv32.q32

// ============================================================================
// Greater Equal: int >= int -> bool
// ============================================================================

bool test_int_greater_equal_simple() {
    // Simple greater than or equal comparison
    return 5 >= 3;
}

// run: test_int_greater_equal_simple() == true

bool test_int_greater_equal_equal() {
    return 5 >= 5;
}

// run: test_int_greater_equal_equal() == true

bool test_int_greater_equal_less() {
    return 3 >= 5;
}

// run: test_int_greater_equal_less() == false

bool test_int_greater_equal_negative() {
    return (-3) >= (-5);
}

// run: test_int_greater_equal_negative() == true

bool test_int_greater_equal_negative_equal() {
    return (-3) >= (-3);
}

// run: test_int_greater_equal_negative_equal() == true

bool test_int_greater_equal_mixed_sign() {
    return 3 >= (-2);
}

// run: test_int_greater_equal_mixed_sign() == true

bool test_int_greater_equal_variables() {
    int a = 15;
    int b = 10;
    return a >= b;
}

// run: test_int_greater_equal_variables() == true

bool test_int_greater_equal_variables_equal() {
    int a = 10;
    int b = 10;
    return a >= b;
}

// run: test_int_greater_equal_variables_equal() == true

bool test_int_greater_equal_expressions() {
    return (6 - 1) >= (2 + 3);
}

// run: test_int_greater_equal_expressions() == true

bool test_int_greater_equal_zero() {
    return 0 >= 0;
}

// run: test_int_greater_equal_zero() == true
