// test run
// target riscv32.q32

// ============================================================================
// Continue in while loops (jumps to condition)
// ============================================================================

int test_continue_while_loop_skip() {
    int sum = 0;
    int i = 0;
    while (i < 5) {
        if (i == 2) {
            i = i + 1;
            continue;
        }
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_continue_while_loop_skip() == 8

int test_continue_while_loop_multiple() {
    int sum = 0;
    int i = 0;
    while (i < 10) {
        if (i % 2 == 0) {
            i = i + 1;
            continue;
        }
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_continue_while_loop_multiple() == 25

int test_continue_while_loop_early() {
    int sum = 0;
    int i = 0;
    while (i < 5) {
        if (i < 2) {
            i = i + 1;
            continue;
        }
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_continue_while_loop_early() == 9

