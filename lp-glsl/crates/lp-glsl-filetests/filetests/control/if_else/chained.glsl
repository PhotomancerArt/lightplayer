// test run
// target riscv32.q32

// ============================================================================
// If-else-if chains
// ============================================================================

int test_if_else_if_first() {
    int x = 0;
    if (true) {
        x = 10;
    } else if (true) {
        x = 20;
    } else {
        x = 30;
    }
    return x;
}

// run: test_if_else_if_first() == 10

int test_if_else_if_second() {
    int x = 0;
    if (false) {
        x = 10;
    } else if (true) {
        x = 20;
    } else {
        x = 30;
    }
    return x;
}

// run: test_if_else_if_second() == 20

int test_if_else_if_else() {
    int x = 0;
    if (false) {
        x = 10;
    } else if (false) {
        x = 20;
    } else {
        x = 30;
    }
    return x;
}

// run: test_if_else_if_else() == 30

int test_if_else_if_chain_three() {
    int x = 0;
    if (false) {
        x = 10;
    } else if (false) {
        x = 20;
    } else if (true) {
        x = 30;
    } else {
        x = 40;
    }
    return x;
}

// run: test_if_else_if_chain_three() == 30

int test_if_else_if_conditional() {
    int x = 0;
    if (5 > 10) {
        x = 10;
    } else if (10 > 5) {
        x = 20;
    } else {
        x = 30;
    }
    return x;
}

// run: test_if_else_if_conditional() == 20





