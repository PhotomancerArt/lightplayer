// test run
// target riscv32.q32

// ============================================================================
// Do-while loops nested in while loops
// ============================================================================

int test_do_while_in_while_loop() {
    int sum = 0;
    int i = 0;
    while (i < 3) {
        int j = 0;
        do {
            sum = sum + i + j;
            j = j + 1;
        } while (j < 2);
        i = i + 1;
    }
    return sum;
}

// run: test_do_while_in_while_loop() == 9

int test_do_while_in_while_loop_continue() {
    int sum = 0;
    int i = 0;
    while (i < 3) {
        int j = 0;
        do {
            if (j == 1) {
                j = j + 1;
                continue;
            }
            sum = sum + 1;
            j = j + 1;
        } while (j < 5);
        i = i + 1;
    }
    return sum;
}

// run: test_do_while_in_while_loop_continue() == 12




