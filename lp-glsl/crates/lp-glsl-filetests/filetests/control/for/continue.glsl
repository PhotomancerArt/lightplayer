// test run
// target riscv32.q32

// ============================================================================
// Continue in for loops (jumps to loop-expression)
// ============================================================================

int test_continue_for_loop_skip() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        if (i == 2) {
            continue;
        }
        sum = sum + i;
    }
    return sum;
}

// run: test_continue_for_loop_skip() == 8

int test_continue_for_loop_multiple() {
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        if (i % 2 == 0) {
            continue;
        }
        sum = sum + i;
    }
    return sum;
}

// run: test_continue_for_loop_multiple() == 25

int test_continue_for_loop_early() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        if (i < 2) {
            continue;
        }
        sum = sum + i;
    }
    return sum;
}

// run: test_continue_for_loop_early() == 9

int test_continue_for_loop_all() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        continue;
        sum = sum + i;
    }
    return sum;
}

// run: test_continue_for_loop_all() == 0

