// test run
// target riscv32.q32

// ============================================================================
// If statements inside while loops
// ============================================================================

int test_if_in_while_loop() {
    int sum = 0;
    int i = 0;
    while (i < 5) {
        if (i % 2 == 0) {
            sum = sum + i;
        }
        i = i + 1;
    }
    return sum;
}

// run: test_if_in_while_loop() == 6




