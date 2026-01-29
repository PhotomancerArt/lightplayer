// test run
// target riscv32.q32

// ============================================================================
// Break in for loops
// ============================================================================

int test_break_for_loop_early() {
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        if (i >= 5) {
            break;
        }
        sum = sum + i;
    }
    return sum;
}

// run: test_break_for_loop_early() == 10

int test_break_for_loop_condition() {
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        sum = sum + i;
        if (sum >= 10) {
            break;
        }
    }
    return sum;
}

// run: test_break_for_loop_condition() == 10

int test_break_for_loop_immediate() {
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        break;
        sum = sum + i;
    }
    return sum;
}

// run: test_break_for_loop_immediate() == 0

int test_break_for_loop_nested_break() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        for (int j = 0; j < 5; j++) {
            if (j >= 2) {
                break;
            }
            sum = sum + 1;
        }
    }
    return sum;
}

// run: test_break_for_loop_nested_break() == 10

