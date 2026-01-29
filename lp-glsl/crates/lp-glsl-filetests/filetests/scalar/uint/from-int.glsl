// test run
// target riscv32.q32

// ============================================================================
// Constructor: uint(int) - preserves value if non-negative (undefined for negative)
// ============================================================================

uint test_uint_from_int_zero() {
    int i = 0;
    return uint(i);
}

// run: test_uint_from_int_zero() == 0u

uint test_uint_from_int_positive() {
    int i = 42;
    return uint(i);
}

// run: test_uint_from_int_positive() == 42u

uint test_uint_from_int_one() {
    int i = 1;
    return uint(i);
}

// run: test_uint_from_int_one() == 1u

uint test_uint_from_int_literal_zero() {
    return uint(0);
}

// run: test_uint_from_int_literal_zero() == 0u

uint test_uint_from_int_literal_positive() {
    return uint(100);
}

// run: test_uint_from_int_literal_positive() == 100u

uint test_uint_from_int_expression() {
    int a = 10;
    int b = 3;
    return uint(a - b);
}

// run: test_uint_from_int_expression() == 7u

uint test_uint_from_int_expression_zero() {
    int a = 5;
    int b = 5;
    return uint(a - b);
}

// run: test_uint_from_int_expression_zero() == 0u

uint test_uint_from_int_in_range() {
    int i = 2147483647;  // INT_MAX (fits in uint)
    return uint(i);
}

// run: test_uint_from_int_in_range() == 2147483647u

