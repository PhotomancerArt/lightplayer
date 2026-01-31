// test run
// target riscv32.q32

// ============================================================================
// While loops nested in for loops
// ============================================================================

int test_while_in_for_loop() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        int j = 0;
        while (j < 2) {
            sum = sum + i + j;
            j = j + 1;
        }
    }
    return sum;
}

// run: test_while_in_for_loop() == 9

int test_while_in_for_loop_break() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        int j = 0;
        while (j < 5) {
            if (j >= 2) {
                break;
            }
            sum = sum + 1;
            j = j + 1;
        }
    }
    return sum;
}

// run: test_while_in_for_loop_break() == 6




