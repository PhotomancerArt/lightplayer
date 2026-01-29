// test run
// target riscv32.q32

// ============================================================================
// Logical AND: && operator - operates on scalar boolean expressions
// ============================================================================

bool test_bool_and_true_true() {
    return true && true;
}

// run: test_bool_and_true_true() == true

bool test_bool_and_true_false() {
    return true && false;
}

// run: test_bool_and_true_false() == false

bool test_bool_and_false_true() {
    return false && true;
}

// run: test_bool_and_false_true() == false

bool test_bool_and_false_false() {
    return false && false;
}

// run: test_bool_and_false_false() == false

bool test_bool_and_variables() {
    bool a = true;
    bool b = false;
    return a && b;
}

// run: test_bool_and_variables() == false

bool test_bool_and_complex() {
    bool a = true;
    bool b = true;
    bool c = false;
    return (a && b) && c;
}

// run: test_bool_and_complex() == false

bool test_bool_and_nested() {
    bool a = true;
    bool b = true;
    bool c = true;
    return a && (b && c);
}

// run: test_bool_and_nested() == true

