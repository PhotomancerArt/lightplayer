// test run
// target riscv32.q32

// ============================================================================
// While loops nested in do-while loops
// ============================================================================

int test_while_in_do_while_loop() {
    int sum = 0;
    int i = 0;
    do {
        int j = 0;
        while (j < 2) {
            sum = sum + i + j;
            j = j + 1;
        }
        i = i + 1;
    } while (i < 3);
    return sum;
}

// run: test_while_in_do_while_loop() == 9

int test_while_in_do_while_loop_continue() {
    int sum = 0;
    int i = 0;
    do {
        int j = 0;
        while (j < 5) {
            if (j == 2) {
                j = j + 1;
                continue;
            }
            sum = sum + 1;
            j = j + 1;
        }
        i = i + 1;
    } while (i < 3);
    return sum;
}

// run: test_while_in_do_while_loop_continue() == 12




