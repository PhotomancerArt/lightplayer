// test run
// target riscv32.q32

// ============================================================================
// Divide: int / int -> int (truncates toward zero)
// ============================================================================

int test_int_divide_positive_positive() {
    // Division with positive integers (truncates toward zero)
    return 10 / 3;
}

// run: test_int_divide_positive_positive() == 3

int test_int_divide_positive_negative() {
    return 10 / (-3);
}

// run: test_int_divide_positive_negative() == -3

int test_int_divide_negative_negative() {
    return (-10) / (-3);
}

// run: test_int_divide_negative_negative() == 3

int test_int_divide_by_one() {
    return 42 / 1;
}

// run: test_int_divide_by_one() == 42

int test_int_divide_variables() {
    int a = 20;
    int b = 4;
    return a / b;
}

// run: test_int_divide_variables() == 5

int test_int_divide_expressions() {
    return (24 / 3) / (8 / 2);
}

// run: test_int_divide_expressions() == 2

int test_int_divide_in_assignment() {
    int result = 15;
    result = result / 3;
    return result;
}

// run: test_int_divide_in_assignment() == 5

int test_int_divide_exact() {
    return 18 / 6;
}

// run: test_int_divide_exact() == 3

int test_int_divide_remainder() {
    return 17 / 5;
}

// run: test_int_divide_remainder() == 3
