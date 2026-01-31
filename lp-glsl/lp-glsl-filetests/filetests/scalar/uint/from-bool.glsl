// test run
// target riscv32.q32

// ============================================================================
// Constructor: uint(bool) - converts bool to uint (false -> 0u, true -> 1u)
// ============================================================================

uint test_uint_from_bool_false() {
    bool b = false;
    return uint(b);
}

// run: test_uint_from_bool_false() == 0u

uint test_uint_from_bool_true() {
    bool b = true;
    return uint(b);
}

// run: test_uint_from_bool_true() == 1u

uint test_uint_from_bool_literal_false() {
    return uint(false);
}

// run: test_uint_from_bool_literal_false() == 0u

uint test_uint_from_bool_literal_true() {
    return uint(true);
}

// run: test_uint_from_bool_literal_true() == 1u

uint test_uint_from_bool_expression() {
    bool a = true;
    bool b = false;
    return uint(a && b);
}

// run: test_uint_from_bool_expression() == 0u

uint test_uint_from_bool_expression_true() {
    bool a = true;
    bool b = true;
    return uint(a && b);
}

// run: test_uint_from_bool_expression_true() == 1u

uint test_uint_from_bool_nested() {
    bool a = true;
    return uint(uint(a));
}

// run: test_uint_from_bool_nested() == 1u

