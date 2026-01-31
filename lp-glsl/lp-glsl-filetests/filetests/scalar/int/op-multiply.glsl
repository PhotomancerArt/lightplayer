// test run
// target riscv32.q32

// ============================================================================
// Multiply: int * int -> int
// ============================================================================

int test_int_multiply_positive_positive() {
    // Multiplication with positive integers
    return 6 * 7;
}

// run: test_int_multiply_positive_positive() == 42

int test_int_multiply_positive_negative() {
    return 5 * (-3);
}

// run: test_int_multiply_positive_negative() == -15

int test_int_multiply_negative_negative() {
    return (-4) * (-5);
}

// run: test_int_multiply_negative_negative() == 20

int test_int_multiply_by_zero() {
    return 123 * 0;
}

// run: test_int_multiply_by_zero() == 0

int test_int_multiply_by_one() {
    return 42 * 1;
}

// run: test_int_multiply_by_one() == 42

int test_int_multiply_variables() {
    int a = 8;
    int b = 9;
    return a * b;
}

// run: test_int_multiply_variables() == 72

int test_int_multiply_expressions() {
    return (3 * 4) * (2 * 5);
}

// run: test_int_multiply_expressions() == 120

int test_int_multiply_in_assignment() {
    int result = 6;
    result = result * 7;
    return result;
}

// run: test_int_multiply_in_assignment() == 42

int test_int_multiply_large_numbers() {
    return 1000 * 2000;
}

// run: test_int_multiply_large_numbers() == 2000000

int test_int_multiply_small_numbers() {
    return 2 * 3;
}

// run: test_int_multiply_small_numbers() == 6
