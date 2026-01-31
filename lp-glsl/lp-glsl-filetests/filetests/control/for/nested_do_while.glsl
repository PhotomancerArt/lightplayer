// test run
// target riscv32.q32

// ============================================================================
// Do-while loops nested in for loops
// ============================================================================

int test_do_while_in_for_loop() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        int j = 0;
        do {
            sum = sum + i + j;
            j = j + 1;
        } while (j < 2);
    }
    return sum;
}

// run: test_do_while_in_for_loop() == 9

int test_do_while_in_for_loop_break() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        int j = 0;
        do {
            if (j >= 2) {
                break;
            }
            sum = sum + 1;
            j = j + 1;
        } while (j < 5);
    }
    return sum;
}

// run: test_do_while_in_for_loop_break() == 6




