// test run
// target riscv32.q32

// ============================================================================
// Conversion: uint(bool) - converts bool to uint (false -> 0u, true -> 1u)
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
    return uint(a || b);
}

// run: test_uint_from_bool_expression() == 1u

uint test_uint_from_bool_expression_false() {
    bool a = false;
    bool b = false;
    return uint(a || b);
}

// run: test_uint_from_bool_expression_false() == 0u

uint test_uint_from_bool_not() {
    bool a = true;
    return uint(!a);
}

// run: test_uint_from_bool_not() == 0u

uint test_uint_from_bool_comparison() {
    int x = 2;
    int y = 5;
    return uint(x < y);
}

// run: test_uint_from_bool_comparison() == 1u

