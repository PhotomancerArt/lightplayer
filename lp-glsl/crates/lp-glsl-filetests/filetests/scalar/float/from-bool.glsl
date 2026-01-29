// test run
// target riscv32.q32

// ============================================================================
// Constructor: float(bool) - converts bool to float (false -> 0.0, true -> 1.0)
// ============================================================================

float test_float_from_bool_false() {
    bool b = false;
    return float(b);
}

// run: test_float_from_bool_false() ~= 0.0

float test_float_from_bool_true() {
    bool b = true;
    return float(b);
}

// run: test_float_from_bool_true() ~= 1.0

float test_float_from_bool_literal_false() {
    return float(false);
}

// run: test_float_from_bool_literal_false() ~= 0.0

float test_float_from_bool_literal_true() {
    return float(true);
}

// run: test_float_from_bool_literal_true() ~= 1.0

float test_float_from_bool_expression() {
    bool a = true;
    bool b = false;
    return float(a && b);
}

// run: test_float_from_bool_expression() ~= 0.0

float test_float_from_bool_expression_true() {
    bool a = true;
    bool b = true;
    return float(a && b);
}

// run: test_float_from_bool_expression_true() ~= 1.0

float test_float_from_bool_nested() {
    bool a = true;
    return float(float(a));
}

// run: test_float_from_bool_nested() ~= 1.0

