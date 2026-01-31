// test run
// target riscv32.q32

// ============================================================================
// Subtract: int - int -> int
// ============================================================================

int test_int_subtract_positive_positive() {
    // Subtraction with positive integers
    return 10 - 3;
}

// run: test_int_subtract_positive_positive() == 7

int test_int_subtract_positive_negative() {
    return 10 - (-4);
}

// run: test_int_subtract_positive_negative() == 14

int test_int_subtract_negative_negative() {
    return (-3) - (-7);
}

// run: test_int_subtract_negative_negative() == 4

int test_int_subtract_zero() {
    return 42 - 0;
}

// run: test_int_subtract_zero() == 42

int test_int_subtract_variables() {
    int a = 50;
    int b = 15;
    return a - b;
}

// run: test_int_subtract_variables() == 35

int test_int_subtract_expressions() {
    return (20 - 5) - (8 - 3);
}

// run: test_int_subtract_expressions() == 10

int test_int_subtract_in_assignment() {
    int result = 20;
    result = result - 8;
    return result;
}

// run: test_int_subtract_in_assignment() == 12

int test_int_subtract_large_numbers() {
    return 500000 - 200000;
}

// run: test_int_subtract_large_numbers() == 300000

int test_int_subtract_small_numbers() {
    return 5 - 2;
}

// run: test_int_subtract_small_numbers() == 3
