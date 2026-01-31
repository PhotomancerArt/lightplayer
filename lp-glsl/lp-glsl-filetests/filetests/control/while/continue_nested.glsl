// test run
// target riscv32.q32

// ============================================================================
// Continue in nested while loops
// ============================================================================

int test_continue_nested_while_inner() {
    int sum = 0;
    int i = 0;
    while (i < 3) {
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
    }
    return sum;
}

// run: test_continue_nested_while_inner() == 12
