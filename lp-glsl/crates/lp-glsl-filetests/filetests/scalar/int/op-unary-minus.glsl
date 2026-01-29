// test run
// target riscv32.q32

// ============================================================================
// Unary Minus: -int -> int (negation)
// ============================================================================

int test_int_unary_minus_positive() {
    // Unary minus (negation)
    return -5;
}

// run: test_int_unary_minus_positive() == -5

int test_int_unary_minus_negative() {
    return -(-10);
}

// run: test_int_unary_minus_negative() == 10

int test_int_unary_minus_zero() {
    return -0;
}

// run: test_int_unary_minus_zero() == 0

int test_int_unary_minus_variable() {
    int a = 42;
    return -a;
}

// run: test_int_unary_minus_variable() == -42

int test_int_unary_minus_expression() {
    return -(5 + 3);
}

// run: test_int_unary_minus_expression() == -8

int test_int_unary_minus_double_negation() {
    return -(-25);
}

// run: test_int_unary_minus_double_negation() == 25

int test_int_unary_minus_in_arithmetic() {
    return 10 + (-5);
}

// run: test_int_unary_minus_in_arithmetic() == 5

int test_int_unary_minus_large_number() {
    return -100000;
}

// run: test_int_unary_minus_large_number() == -100000

int test_int_unary_minus_small_number() {
    return -1;
}

// run: test_int_unary_minus_small_number() == -1
