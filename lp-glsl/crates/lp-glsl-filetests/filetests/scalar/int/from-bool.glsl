// test run
// target riscv32.q32

// ============================================================================
// Constructor: int(bool) - converts bool to int (false -> 0, true -> 1)
// ============================================================================

int test_int_from_bool_false() {
    bool b = false;
    return int(b);
}

// run: test_int_from_bool_false() == 0

int test_int_from_bool_true() {
    bool b = true;
    return int(b);
}

// run: test_int_from_bool_true() == 1

int test_int_from_bool_literal_false() {
    return int(false);
}

// run: test_int_from_bool_literal_false() == 0

int test_int_from_bool_literal_true() {
    return int(true);
}

// run: test_int_from_bool_literal_true() == 1

int test_int_from_bool_expression() {
    bool a = true;
    bool b = false;
    return int(a && b);
}

// run: test_int_from_bool_expression() == 0

int test_int_from_bool_expression_true() {
    bool a = true;
    bool b = true;
    return int(a && b);
}

// run: test_int_from_bool_expression_true() == 1

int test_int_from_bool_nested() {
    bool a = true;
    return int(int(a));
}

// run: test_int_from_bool_nested() == 1

