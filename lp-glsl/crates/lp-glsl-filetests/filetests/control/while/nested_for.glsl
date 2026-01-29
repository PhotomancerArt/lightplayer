// test run
// target riscv32.q32

// ============================================================================
// For loops nested in while loops
// ============================================================================

int test_for_in_while_loop() {
    int sum = 0;
    int i = 0;
    while (i < 3) {
        for (int j = 0; j < 2; j++) {
            sum = sum + i + j;
        }
        i = i + 1;
    }
    return sum;
}

// run: test_for_in_while_loop() == 9

int test_for_in_while_loop_continue() {
    int sum = 0;
    int i = 0;
    while (i < 3) {
        for (int j = 0; j < 5; j++) {
            if (j == 2) {
                continue;
            }
            sum = sum + 1;
        }
        i = i + 1;
    }
    return sum;
}

// run: test_for_in_while_loop_continue() == 12




