// test run
// target riscv32.q32

// ============================================================================
// For loops nested in do-while loops
// ============================================================================

int test_for_in_do_while_loop() {
    int sum = 0;
    int i = 0;
    do {
        for (int j = 0; j < 2; j++) {
            sum = sum + i + j;
        }
        i = i + 1;
    } while (i < 3);
    return sum;
}

// run: test_for_in_do_while_loop() == 9

int test_for_in_do_while_loop_break() {
    int sum = 0;
    int i = 0;
    do {
        for (int j = 0; j < 5; j++) {
            if (j >= 2) {
                break;
            }
            sum = sum + 1;
        }
        i = i + 1;
    } while (i < 3);
    return sum;
}

// run: test_for_in_do_while_loop_break() == 6




