// test run
// target riscv32.q32

// ============================================================================
// Constructor: bool(uint) - converts uint to bool (0u -> false, non-zero -> true)
// ============================================================================

bool test_bool_from_uint_zero() {
    uint u = 0u;
    return bool(u);
}

// run: test_bool_from_uint_zero() == false

bool test_bool_from_uint_positive() {
    uint u = 42u;
    return bool(u);
}

// run: test_bool_from_uint_positive() == true

bool test_bool_from_uint_one() {
    uint u = 1u;
    return bool(u);
}

// run: test_bool_from_uint_one() == true

bool test_bool_from_uint_literal_zero() {
    return bool(0u);
}

// run: test_bool_from_uint_literal_zero() == false

bool test_bool_from_uint_literal_nonzero() {
    return bool(7u);
}

// run: test_bool_from_uint_literal_nonzero() == true

bool test_bool_from_uint_expression() {
    uint a = 10u;
    uint b = 3u;
    return bool(a - b);
}

// run: test_bool_from_uint_expression() == true

bool test_bool_from_uint_expression_zero() {
    uint a = 8u;
    uint b = 8u;
    return bool(a - b);
}

// run: test_bool_from_uint_expression_zero() == false

bool test_bool_from_uint_large() {
    uint u = 4294967295u;  // Max uint value
    return bool(u);
}

// run: test_bool_from_uint_large() == true

