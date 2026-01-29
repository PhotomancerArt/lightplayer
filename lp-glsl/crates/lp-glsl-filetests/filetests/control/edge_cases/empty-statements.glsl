// test run
// target riscv32.q32

// ============================================================================
// Empty statements in control flow
// ============================================================================

int test_if_empty_statement() {
    int x = 5;
    if (true);
    return x;
}

// run: test_if_empty_statement() == 5

int test_if_else_empty() {
    int x = 5;
    if (true) {
        // Empty block
    } else {
        x = 10;
    }
    return x;
}

// run: test_if_else_empty() == 5

int test_for_loop_empty_body() {
    int i = 0;
    for (i = 0; i < 3; i++);
    return i;
}

// run: test_for_loop_empty_body() == 3

int test_while_loop_empty_body() {
    int i = 0;
    while (i < 3) {
        i = i + 1;
    }
    return i;
}

// run: test_while_loop_empty_body() == 3





