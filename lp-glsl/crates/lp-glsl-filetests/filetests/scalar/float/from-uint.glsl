// test run
// target riscv32.q32

// ============================================================================
// Constructor: float(uint) - converts uint to float
// ============================================================================

float test_float_from_uint_zero() {
    uint u = 0u;
    return float(u);
}

// run: test_float_from_uint_zero() ~= 0.0

float test_float_from_uint_positive() {
    uint u = 42u;
    return float(u);
}

// run: test_float_from_uint_positive() ~= 42.0

float test_float_from_uint_one() {
    uint u = 1u;
    return float(u);
}

// run: test_float_from_uint_one() ~= 1.0

float test_float_from_uint_literal_zero() {
    return float(0u);
}

// run: test_float_from_uint_literal_zero() ~= 0.0

float test_float_from_uint_literal_positive() {
    return float(100u);
}

// run: test_float_from_uint_literal_positive() ~= 100.0

float test_float_from_uint_expression() {
    uint a = 10u;
    uint b = 3u;
    return float(a - b);
}

// run: test_float_from_uint_expression() ~= 7.0

float test_float_from_uint_expression_zero() {
    uint a = 5u;
    uint b = 5u;
    return float(a - b);
}

// run: test_float_from_uint_expression_zero() ~= 0.0

float test_float_from_uint_large() {
    uint u = 4294967295u;  // UINT_MAX - clamped to fixed16x16 max
    return float(u);
}

// run: test_float_from_uint_large() ~= 32767.0

