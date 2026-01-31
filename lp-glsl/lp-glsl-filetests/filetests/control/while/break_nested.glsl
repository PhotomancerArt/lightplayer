// test run
// target riscv32.q32

// ============================================================================
// Break in nested while loops (breaks inner loop)
// ============================================================================

int test_break_nested_while_inner() {
    int sum = 0;
    int i = 0;
    while (i < 3) {
        int j = 0;
        while (j < 5) {
            if (j >= 2) {
                break;
            }
            sum = sum + 1;
            j = j + 1;
        }
        i = i + 1;
    }
    return sum;
}

// run: test_break_nested_while_inner() == 6
