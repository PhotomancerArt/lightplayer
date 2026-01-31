// test run
// target riscv32.q32

// ============================================================================
// Break in while loops
// ============================================================================

int test_break_while_loop_early() {
    int sum = 0;
    int i = 0;
    while (i < 10) {
        if (i >= 5) {
            break;
        }
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_break_while_loop_early() == 10

int test_break_while_loop_condition() {
    int sum = 0;
    int i = 0;
    while (i < 10) {
        sum = sum + i;
        if (sum >= 10) {
            break;
        }
        i = i + 1;
    }
    return sum;
}

// run: test_break_while_loop_condition() == 10

int test_break_while_loop_immediate() {
    int sum = 0;
    int i = 0;
    while (i < 10) {
        break;
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_break_while_loop_immediate() == 0

int test_break_while_loop_nested_break() {
    int sum = 0;
    int i = 0;
    while (i < 5) {
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

// run: test_break_while_loop_nested_break() == 10

