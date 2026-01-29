// test run
// target riscv32.q32

// ============================================================================
// Constructor: int(uint) - preserves value if in range
// ============================================================================

int test_int_from_uint_zero() {
    uint u = 0u;
    return int(u);
}

// run: test_int_from_uint_zero() == 0

int test_int_from_uint_positive() {
    uint u = 42u;
    return int(u);
}

// run: test_int_from_uint_positive() == 42

int test_int_from_uint_one() {
    uint u = 1u;
    return int(u);
}

// run: test_int_from_uint_one() == 1

int test_int_from_uint_literal_zero() {
    return int(0u);
}

// run: test_int_from_uint_literal_zero() == 0

int test_int_from_uint_literal_positive() {
    return int(100u);
}

// run: test_int_from_uint_literal_positive() == 100

int test_int_from_uint_expression() {
    uint a = 10u;
    uint b = 3u;
    return int(a - b);
}

// run: test_int_from_uint_expression() == 7

int test_int_from_uint_expression_zero() {
    uint a = 5u;
    uint b = 5u;
    return int(a - b);
}

// run: test_int_from_uint_expression_zero() == 0

int test_int_from_uint_in_range() {
    uint u = 2147483647u;  // INT_MAX
    return int(u);
}

// run: test_int_from_uint_in_range() == 2147483647

