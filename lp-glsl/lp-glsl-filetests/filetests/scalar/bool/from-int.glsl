// test run
// target riscv32.q32

// ============================================================================
// Constructor: bool(int) - converts int to bool (0 -> false, non-zero -> true)
// ============================================================================

bool test_bool_from_int_zero() {
    int i = 0;
    return bool(i);
}

// run: test_bool_from_int_zero() == false

bool test_bool_from_int_positive() {
    int i = 42;
    return bool(i);
}

// run: test_bool_from_int_positive() == true

bool test_bool_from_int_negative() {
    int i = -10;
    return bool(i);
}

// run: test_bool_from_int_negative() == true

bool test_bool_from_int_one() {
    int i = 1;
    return bool(i);
}

// run: test_bool_from_int_one() == true

bool test_bool_from_int_literal_zero() {
    return bool(0);
}

// run: test_bool_from_int_literal_zero() == false

bool test_bool_from_int_literal_nonzero() {
    return bool(5);
}

// run: test_bool_from_int_literal_nonzero() == true

bool test_bool_from_int_expression() {
    int a = 3;
    int b = 2;
    return bool(a - b);
}

// run: test_bool_from_int_expression() == true

bool test_bool_from_int_expression_zero() {
    int a = 5;
    int b = 5;
    return bool(a - b);
}

// run: test_bool_from_int_expression_zero() == false

