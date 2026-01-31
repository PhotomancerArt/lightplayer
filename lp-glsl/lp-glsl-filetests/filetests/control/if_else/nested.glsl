// test run
// target riscv32.q32

// ============================================================================
// Nested if-else chains
// ============================================================================

int test_if_else_nested_both_if() {
    int x = 0;
    if (true) {
        if (true) {
            x = 10;
        } else {
            x = 20;
        }
    } else {
        x = 30;
    }
    return x;
}

// run: test_if_else_nested_both_if() == 10

int test_if_else_nested_inner_else() {
    int x = 0;
    if (true) {
        if (false) {
            x = 10;
        } else {
            x = 20;
        }
    } else {
        x = 30;
    }
    return x;
}

// run: test_if_else_nested_inner_else() == 20

int test_if_else_nested_outer_else() {
    int x = 0;
    if (false) {
        if (true) {
            x = 10;
        } else {
            x = 20;
        }
    } else {
        x = 30;
    }
    return x;
}

// run: test_if_else_nested_outer_else() == 30

int test_if_else_triple_nested() {
    int x = 0;
    if (true) {
        if (true) {
            if (true) {
                x = 100;
            } else {
                x = 200;
            }
        } else {
            x = 300;
        }
    } else {
        x = 400;
    }
    return x;
}

// run: test_if_else_triple_nested() == 100





