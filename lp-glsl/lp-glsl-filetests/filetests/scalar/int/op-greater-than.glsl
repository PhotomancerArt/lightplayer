// test run
// target riscv32.q32

// ============================================================================
// Greater Than: int > int -> bool
// ============================================================================

bool test_int_greater_than_simple() {
    // Simple greater than comparison
    return 5 > 3;
}

// run: test_int_greater_than_simple() == true

bool test_int_greater_than_equal() {
    return 5 > 5;
}

// run: test_int_greater_than_equal() == false

bool test_int_greater_than_negative() {
    return (-3) > (-5);
}

// run: test_int_greater_than_negative() == true

bool test_int_greater_than_mixed_sign() {
    return 3 > (-2);
}

// run: test_int_greater_than_mixed_sign() == true

bool test_int_greater_than_from_zero() {
    return 0 > (-1);
}

// run: test_int_greater_than_from_zero() == true

bool test_int_greater_than_to_zero() {
    return 1 > 0;
}

// run: test_int_greater_than_to_zero() == true

bool test_int_greater_than_variables() {
    int a = 15;
    int b = 10;
    return a > b;
}

// run: test_int_greater_than_variables() == true

bool test_int_greater_than_expressions() {
    return (6 - 1) > (2 + 3);
}

// run: test_int_greater_than_expressions() == false

bool test_int_greater_than_large_numbers() {
    return 200000 > 100000;
}

// run: test_int_greater_than_large_numbers() == true

bool test_int_greater_than_small_numbers() {
    return 2 > 1;
}

// run: test_int_greater_than_small_numbers() == true
