// test run
// target riscv32.q32

// ============================================================================
// Not Equal: != operator - operates on scalar boolean expressions
// ============================================================================

bool test_bool_not_equal_true_true() {
    return true != true;
}

// run: test_bool_not_equal_true_true() == false

bool test_bool_not_equal_true_false() {
    return true != false;
}

// run: test_bool_not_equal_true_false() == true

bool test_bool_not_equal_false_true() {
    return false != true;
}

// run: test_bool_not_equal_false_true() == true

bool test_bool_not_equal_false_false() {
    return false != false;
}

// run: test_bool_not_equal_false_false() == false

bool test_bool_not_equal_variables_same() {
    bool a = true;
    bool b = true;
    return a != b;
}

// run: test_bool_not_equal_variables_same() == false

bool test_bool_not_equal_variables_different() {
    bool a = true;
    bool b = false;
    return a != b;
}

// run: test_bool_not_equal_variables_different() == true

bool test_bool_not_equal_self() {
    bool a = false;
    return a != a;
}

// run: test_bool_not_equal_self() == false

bool test_bool_not_equal_after_assignment() {
    bool a = true;
    bool b = false;
    b = a;
    return a != b;
}

// run: test_bool_not_equal_after_assignment() == false

bool test_bool_not_equal_in_expression() {
    bool a = true;
    bool b = false;
    bool c = true;
    return (a != b) && (b != c);
}

// run: test_bool_not_equal_in_expression() == true

