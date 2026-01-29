// test run
// target riscv32.q32

// ============================================================================
// Add: int + int -> int
// ============================================================================

int test_int_add_positive_positive() {
    // Addition with positive integers
    return 5 + 3;
}

// run: test_int_add_positive_positive() == 8

int test_int_add_positive_negative() {
    return 10 + (-4);
}

// run: test_int_add_positive_negative() == 6

int test_int_add_negative_negative() {
    return (-3) + (-7);
}

// run: test_int_add_negative_negative() == -10

int test_int_add_zero() {
    return 42 + 0;
}

// run: test_int_add_zero() == 42

int test_int_add_variables() {
    int a = 15;
    int b = 27;
    return a + b;
}

// run: test_int_add_variables() == 42

int test_int_add_expressions() {
    return (8 + 4) + (6 + 2);
}

// run: test_int_add_expressions() == 20

int test_int_add_in_assignment() {
    int result = 5;
    result = result + 10;
    return result;
}

// run: test_int_add_in_assignment() == 15

int test_int_add_large_numbers() {
    return 100000 + 200000;
}

// run: test_int_add_large_numbers() == 300000

int test_int_add_small_numbers() {
    return 1 + 2;
}

// run: test_int_add_small_numbers() == 3
