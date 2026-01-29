// test run
// target riscv32.q32

// ============================================================================
// Constructor: bool(float) - converts float to bool (0.0 -> false, non-zero -> true)
// ============================================================================

bool test_bool_from_float_zero() {
    float f = 0.0;
    return bool(f);
}

// run: test_bool_from_float_zero() == false

bool test_bool_from_float_positive() {
    float f = 3.14;
    return bool(f);
}

// run: test_bool_from_float_positive() == true

bool test_bool_from_float_negative() {
    float f = -2.5;
    return bool(f);
}

// run: test_bool_from_float_negative() == true

bool test_bool_from_float_one() {
    float f = 1.0;
    return bool(f);
}

// run: test_bool_from_float_one() == true

bool test_bool_from_float_small() {
    float f = 0.0001;
    return bool(f);
}

// run: test_bool_from_float_small() == true

bool test_bool_from_float_literal_zero() {
    return bool(0.0);
}

// run: test_bool_from_float_literal_zero() == false

bool test_bool_from_float_literal_nonzero() {
    return bool(2.5);
}

// run: test_bool_from_float_literal_nonzero() == true

bool test_bool_from_float_expression() {
    float a = 5.0;
    float b = 2.0;
    return bool(a - b);
}

// run: test_bool_from_float_expression() == true

bool test_bool_from_float_expression_zero() {
    float a = 4.0;
    float b = 4.0;
    return bool(a - b);
}

// run: test_bool_from_float_expression_zero() == false

