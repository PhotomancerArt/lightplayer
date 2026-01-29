// test run
// target riscv32.q32

// ============================================================================
// Edge cases: Nested boolean operations
// ============================================================================

bool test_bool_nested_and_or() {
    bool a = true;
    bool b = false;
    bool c = true;
    bool d = false;
    return (a && b) || (c && d);
}

// run: test_bool_nested_and_or() == false

bool test_bool_nested_and_or_true() {
    bool a = true;
    bool b = true;
    bool c = false;
    bool d = false;
    return (a && b) || (c && d);
}

// run: test_bool_nested_and_or_true() == true

bool test_bool_nested_multiple_levels() {
    bool a = true;
    bool b = true;
    bool c = false;
    return (a && b) && (c || true);
}

// run: test_bool_nested_multiple_levels() == true

bool test_bool_nested_with_not() {
    bool a = true;
    bool b = false;
    bool c = true;
    return !(a && b) && (c || false);
}

// run: test_bool_nested_with_not() == true

bool test_bool_nested_complex() {
    bool a = true;
    bool b = false;
    bool c = true;
    bool d = true;
    return ((a || b) && (c && d)) || (!a && b);
}

// run: test_bool_nested_complex() == true

bool test_bool_nested_parentheses() {
    bool a = true;
    bool b = false;
    bool c = true;
    return a && (b || c);
}

// run: test_bool_nested_parentheses() == true

bool test_bool_nested_deep() {
    bool a = true;
    bool b = true;
    bool c = false;
    bool d = true;
    return ((a && b) || (c && d)) && (!c || a);
}

// run: test_bool_nested_deep() == true

bool test_bool_nested_mixed_operators() {
    bool a = true;
    bool b = false;
    bool c = true;
    // Operator precedence: && before ||, so we need parentheses to clarify
    return (a && b) || ((c && !b) && (a || c));
}

// run: test_bool_nested_mixed_operators() == true

