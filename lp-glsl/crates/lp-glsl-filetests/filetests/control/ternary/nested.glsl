// test run
// target riscv32.q32

// ============================================================================
// Nested ternary operator tests
// Spec: Ternary operators can be nested
//       Right-to-left associativity affects grouping
// ============================================================================

int test_ternary_nested_basic() {
    bool a = true;
    bool b = false;
    return a ? (b ? 1 : 2) : (b ? 3 : 4);
}

// run: test_ternary_nested_basic() == 2

int test_ternary_nested_all_true() {
    bool a = true;
    bool b = true;
    bool c = true;
    return a ? (b ? (c ? 1 : 2) : 3) : 4;
}

// run: test_ternary_nested_all_true() == 1

int test_ternary_nested_all_false() {
    bool a = false;
    bool b = false;
    bool c = false;
    return a ? (b ? (c ? 1 : 2) : 3) : 4;
}

// run: test_ternary_nested_all_false() == 4

int test_ternary_nested_mixed() {
    bool a = true;
    bool b = false;
    bool c = true;
    return a ? (b ? (c ? 1 : 2) : 3) : 4;
}

// run: test_ternary_nested_mixed() == 3

int test_ternary_nested_right_associative() {
    bool a = true;
    bool b = true;
    bool c = false;
    // Right-to-left associativity: a ? (b ? (c ? 1 : 2) : 3) : 4
    return a ? b ? c ? 1 : 2 : 3 : 4;
}

// run: test_ternary_nested_right_associative() == 2

int test_ternary_nested_with_parentheses() {
    bool a = false;
    bool b = true;
    bool c = false;
    // Explicit parentheses override associativity
    return (a ? b : c) ? 10 : 20;
}

// run: test_ternary_nested_with_parentheses() == 20

int test_ternary_nested_deep() {
    bool a = true;
    bool b = false;
    bool c = true;
    bool d = false;
    return a ? (b ? (c ? (d ? 1 : 2) : 3) : 4) : 5;
}

// run: test_ternary_nested_deep() == 4

int test_ternary_nested_in_expression() {
    bool a = true;
    bool b = false;
    int x = (a ? (b ? 10 : 20) : 30) + 5;
    return x;
}

// run: test_ternary_nested_in_expression() == 25

int test_ternary_nested_as_condition() {
    bool a = true;
    bool b = false;
    // Using nested ternary as condition for another ternary
    return (a ? b : true) ? 100 : 200;
}

// run: test_ternary_nested_as_condition() == 200

int test_ternary_nested_symmetric() {
    bool a = true;
    bool b = false;
    // Symmetric nesting
    return a ? (b ? 1 : 2) : (b ? 3 : 4);
}

// run: test_ternary_nested_symmetric() == 2

int test_ternary_nested_chain() {
    bool a = false;
    bool b = true;
    bool c = false;
    // Chain: a ? b ? c ? 1 : 2 : 3 : 4
    // Groups as: a ? (b ? (c ? 1 : 2) : 3) : 4
    // a=false -> 4
    return a ? b ? c ? 1 : 2 : 3 : 4;
}

// run: test_ternary_nested_chain() == 4





