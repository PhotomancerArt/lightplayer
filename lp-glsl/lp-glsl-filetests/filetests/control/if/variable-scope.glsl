// test run
// target riscv32.q32

// ============================================================================
// Variable scoping within if blocks
// ============================================================================

int test_if_variable_scope_inside() {
    int x = 0;
    if (true) {
        int y = 10;
        x = y;
    }
    return x;
}

// run: test_if_variable_scope_inside() == 10

int test_if_variable_scope_outside() {
    int x = 5;
    if (true) {
        x = 10;
    }
    return x;
}

// run: test_if_variable_scope_outside() == 10

int test_if_variable_shadowing() {
    int x = 5;
    if (true) {
        int x = 10;
        // Inner x shadows outer x, so outer x remains unchanged
    }
    return x;
}

// run: test_if_variable_shadowing() == 5

int test_if_variable_independent() {
    int x = 0;
    if (true) {
        int y = 20;
        x = y;
    }
    // y is out of scope here
    return x;
}

// run: test_if_variable_independent() == 20

int test_if_multiple_blocks() {
    int x = 0;
    if (true) {
        int a = 5;
        x = a;
    }
    if (true) {
        int a = 10;
        x = x + a;
    }
    return x;
}

// run: test_if_multiple_blocks() == 15





