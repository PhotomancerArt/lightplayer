// test run
// target riscv32.q32

// ============================================================================
// Constructor: bool(bool) - identity constructor
// ============================================================================

bool test_bool_from_bool_true() {
    bool b = true;
    return bool(b);
}

// run: test_bool_from_bool_true() == true

bool test_bool_from_bool_false() {
    bool b = false;
    return bool(b);
}

// run: test_bool_from_bool_false() == false

bool test_bool_from_bool_literal_true() {
    return bool(true);
}

// run: test_bool_from_bool_literal_true() == true

bool test_bool_from_bool_literal_false() {
    return bool(false);
}

// run: test_bool_from_bool_literal_false() == false

bool test_bool_from_bool_expression() {
    bool a = true;
    bool b = false;
    return bool(a && b);
}

// run: test_bool_from_bool_expression() == false

bool test_bool_from_bool_nested() {
    bool a = true;
    return bool(bool(a));
}

// run: test_bool_from_bool_nested() == true

bool test_bool_from_bool_after_not() {
    bool a = true;
    return bool(!a);
}

// run: test_bool_from_bool_after_not() == false

bool test_bool_from_bool_self() {
    bool a = false;
    bool b = bool(a);
    return b == a;
}

// run: test_bool_from_bool_self() == true

