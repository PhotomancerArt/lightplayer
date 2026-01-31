// test run
// target riscv32.q32

// ============================================================================
// Less Than: int < int -> bool
// ============================================================================

bool test_int_less_than_simple() {
    // Simple less than comparison
    return 3 < 5;
}

// run: test_int_less_than_simple() == true

bool test_int_less_than_equal() {
    return 5 < 5;
}

// run: test_int_less_than_equal() == false

bool test_int_less_than_negative() {
    return (-5) < (-3);
}

// run: test_int_less_than_negative() == true

bool test_int_less_than_mixed_sign() {
    return (-2) < 3;
}

// run: test_int_less_than_mixed_sign() == true

bool test_int_less_than_from_zero() {
    return (-1) < 0;
}

// run: test_int_less_than_from_zero() == true

bool test_int_less_than_to_zero() {
    return 0 < 1;
}

// run: test_int_less_than_to_zero() == true

bool test_int_less_than_variables() {
    int a = 10;
    int b = 15;
    return a < b;
}

// run: test_int_less_than_variables() == true

bool test_int_less_than_expressions() {
    return (2 + 3) < (6 - 1);
}

// run: test_int_less_than_expressions() == false

bool test_int_less_than_large_numbers() {
    return 100000 < 200000;
}

// run: test_int_less_than_large_numbers() == true

bool test_int_less_than_small_numbers() {
    return 1 < 2;
}

// run: test_int_less_than_small_numbers() == true
