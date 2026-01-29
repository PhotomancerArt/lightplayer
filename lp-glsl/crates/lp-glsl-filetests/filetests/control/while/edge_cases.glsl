// test run
// target riscv32.q32

// ============================================================================
// Break and continue edge cases for while loops
// ============================================================================

int test_break_in_while_with_continue() {
    int sum = 0;
    int i = 0;
    while (i < 10) {
        if (i % 2 == 0) {
            i = i + 1;
            continue;
        }
        if (i >= 7) {
            break;
        }
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_break_in_while_with_continue() == 9
