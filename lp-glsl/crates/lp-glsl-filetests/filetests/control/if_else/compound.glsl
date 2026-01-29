// test run
// target riscv32.q32

// ============================================================================
// If-else with compound statements
// ============================================================================

int test_if_else_compound_if() {
    int x = 0;
    int y = 0;
    if (true) {
        x = 5;
        y = 10;
    } else {
        x = 20;
        y = 30;
    }
    return x + y;
}

// run: test_if_else_compound_if() == 15

int test_if_else_compound_else() {
    int x = 0;
    int y = 0;
    if (false) {
        x = 5;
        y = 10;
    } else {
        x = 20;
        y = 30;
    }
    return x + y;
}

// run: test_if_else_compound_else() == 50

int test_if_else_nested_blocks() {
    int x = 0;
    if (true) {
        {
            int y = 5;
            x = y;
        }
    } else {
        {
            int y = 10;
            x = y;
        }
    }
    return x;
}

// run: test_if_else_nested_blocks() == 5





