// test run
// target riscv32.q32

// ============================================================================
// Constructor: int(float) - truncates fractional part toward zero
// ============================================================================

int test_int_from_float_zero() {
    float f = 0.0;
    return int(f);
}

// run: test_int_from_float_zero() == 0

int test_int_from_float_one() {
    float f = 1.0;
    return int(f);
}

// run: test_int_from_float_one() == 1

int test_int_from_float_positive_truncate() {
    float f = 1.5;
    return int(f);
}

// run: test_int_from_float_positive_truncate() == 1

int test_int_from_float_negative_truncate() {
    float f = -1.5;
    return int(f);
}

// run: test_int_from_float_negative_truncate() == -1

int test_int_from_float_positive_high() {
    float f = 3.9;
    return int(f);
}

// run: test_int_from_float_positive_high() == 3

int test_int_from_float_negative_low() {
    float f = -2.7;
    return int(f);
}

// run: test_int_from_float_negative_low() == -2

int test_int_from_float_literal_zero() {
    return int(0.0);
}

// run: test_int_from_float_literal_zero() == 0

int test_int_from_float_literal_positive() {
    return int(42.0);
}

// run: test_int_from_float_literal_positive() == 42

int test_int_from_float_literal_negative() {
    return int(-10.0);
}

// run: test_int_from_float_literal_negative() == -10

int test_int_from_float_expression() {
    float a = 5.0;
    float b = 2.5;
    return int(a - b);
}

// run: test_int_from_float_expression() == 2

int test_int_from_float_expression_negative() {
    float a = 3.0;
    float b = 5.7;
    return int(a - b);
}

// run: test_int_from_float_expression_negative() == -2

