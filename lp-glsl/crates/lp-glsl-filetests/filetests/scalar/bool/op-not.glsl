// test run
// target riscv32.q32

// ============================================================================
// Logical NOT: ! operator - operates on scalar boolean expressions
// ============================================================================

bool test_bool_not_true() {
    return !true;
}

// run: test_bool_not_true() == false

bool test_bool_not_false() {
    return !false;
}

// run: test_bool_not_false() == true

bool test_bool_not_variable_true() {
    bool a = true;
    return !a;
}

// run: test_bool_not_variable_true() == false

bool test_bool_not_variable_false() {
    bool a = false;
    return !a;
}

// run: test_bool_not_variable_false() == true

bool test_bool_not_double_negation() {
    bool a = true;
    return !!a;
}

// run: test_bool_not_double_negation() == true

bool test_bool_not_triple_negation() {
    bool a = false;
    return !!!a;
}

// run: test_bool_not_triple_negation() == true

bool test_bool_not_in_expression() {
    bool a = true;
    bool b = false;
    return !a && !b;
}

// run: test_bool_not_in_expression() == false

bool test_bool_not_complex() {
    bool a = true;
    bool b = false;
    return !(a && b);
}

// run: test_bool_not_complex() == true

