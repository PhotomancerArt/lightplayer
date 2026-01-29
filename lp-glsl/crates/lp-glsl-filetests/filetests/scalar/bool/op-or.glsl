// test run
// target riscv32.q32

// ============================================================================
// Logical OR: || operator - operates on scalar boolean expressions
// ============================================================================

bool test_bool_or_true_true() {
    return true || true;
}

// run: test_bool_or_true_true() == true

bool test_bool_or_true_false() {
    return true || false;
}

// run: test_bool_or_true_false() == true

bool test_bool_or_false_true() {
    return false || true;
}

// run: test_bool_or_false_true() == true

bool test_bool_or_false_false() {
    return false || false;
}

// run: test_bool_or_false_false() == false

bool test_bool_or_variables() {
    bool a = true;
    bool b = false;
    return a || b;
}

// run: test_bool_or_variables() == true

bool test_bool_or_complex() {
    bool a = false;
    bool b = false;
    bool c = true;
    return (a || b) || c;
}

// run: test_bool_or_complex() == true

bool test_bool_or_nested() {
    bool a = false;
    bool b = true;
    bool c = false;
    return a || (b || c);
}

// run: test_bool_or_nested() == true

bool test_bool_or_all_false() {
    bool a = false;
    bool b = false;
    bool c = false;
    return a || b || c;
}

// run: test_bool_or_all_false() == false

