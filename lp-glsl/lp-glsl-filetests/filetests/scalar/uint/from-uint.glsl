// test run
// target riscv32.q32

// ============================================================================
// Constructor: uint(uint) - identity constructor
// ============================================================================

uint test_uint_from_uint_zero() {
    uint u = 0u;
    return uint(u);
}

// run: test_uint_from_uint_zero() == 0u

uint test_uint_from_uint_positive() {
    uint u = 42u;
    return uint(u);
}

// run: test_uint_from_uint_positive() == 42u

uint test_uint_from_uint_one() {
    uint u = 1u;
    return uint(u);
}

// run: test_uint_from_uint_one() == 1u

uint test_uint_from_uint_literal_zero() {
    return uint(0u);
}

// run: test_uint_from_uint_literal_zero() == 0u

uint test_uint_from_uint_literal_positive() {
    return uint(100u);
}

// run: test_uint_from_uint_literal_positive() == 100u

uint test_uint_from_uint_expression() {
    uint a = 10u;
    uint b = 3u;
    return uint(a - b);
}

// run: test_uint_from_uint_expression() == 7u

uint test_uint_from_uint_expression_zero() {
    uint a = 5u;
    uint b = 5u;
    return uint(a - b);
}

// run: test_uint_from_uint_expression_zero() == 0u

uint test_uint_from_uint_nested() {
    uint u = 42u;
    return uint(uint(u));
}

// run: test_uint_from_uint_nested() == 42u

uint test_uint_from_uint_self() {
    uint a = 100u;
    uint b = uint(a);
    return b;
}

// run: test_uint_from_uint_self() == 100u

uint test_uint_from_uint_max() {
    uint u = 4294967295u;  // UINT_MAX
    return uint(u);
}

// run: test_uint_from_uint_max() == 4294967295u

