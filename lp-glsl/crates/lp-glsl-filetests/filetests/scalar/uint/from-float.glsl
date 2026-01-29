// test run
// target riscv32.q32

// ============================================================================
// Constructor: uint(float) - truncates fractional part toward zero (undefined for negative)
// ============================================================================

uint test_uint_from_float_zero() {
    float f = 0.0;
    return uint(f);
}

// run: test_uint_from_float_zero() == 0u

uint test_uint_from_float_one() {
    float f = 1.0;
    return uint(f);
}

// run: test_uint_from_float_one() == 1u

uint test_uint_from_float_positive_truncate() {
    float f = 1.5;
    return uint(f);
}

// run: test_uint_from_float_positive_truncate() == 1u

uint test_uint_from_float_positive_high() {
    float f = 3.9;
    return uint(f);
}

// run: test_uint_from_float_positive_high() == 3u

uint test_uint_from_float_literal_zero() {
    return uint(0.0);
}

// run: test_uint_from_float_literal_zero() == 0u

uint test_uint_from_float_literal_positive() {
    return uint(42.0);
}

// run: test_uint_from_float_literal_positive() == 42u

uint test_uint_from_float_expression() {
    float a = 5.0;
    float b = 2.5;
    return uint(a - b);
}

// run: test_uint_from_float_expression() == 2u

uint test_uint_from_float_expression_zero() {
    float a = 3.0;
    float b = 3.0;
    return uint(a - b);
}

// run: test_uint_from_float_expression_zero() == 0u

uint test_uint_from_float_large() {
    float f = 32767.0;  // Maximum representable integer in fixed16x16 format
    return uint(f);
}

// run: test_uint_from_float_large() == 32767u

uint test_uint_from_float_negative() {
    float f = -1.0;
    return uint(f);
}

// run: test_uint_from_float_negative() == 4294967295u

uint test_uint_from_float_negative_large() {
    float f = -100.5;
    return uint(f);
}

// run: test_uint_from_float_negative_large() == 4294967196u

uint test_uint_from_float_negative_literal() {
    return uint(-42.0);
}

// run: test_uint_from_float_negative_literal() == 4294967254u

uint test_uint_from_float_negative_expression() {
    float a = 3.0;
    float b = 5.0;
    return uint(a - b);
}

// run: test_uint_from_float_negative_expression() == 4294967294u

