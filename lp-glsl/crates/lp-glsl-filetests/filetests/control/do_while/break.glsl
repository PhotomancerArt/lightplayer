// test run
// target riscv32.q32

// ============================================================================
// Break in do-while loops
// ============================================================================

int test_break_do_while_loop_early() {
    int sum = 0;
    int i = 0;
    do {
        if (i >= 5) {
            break;
        }
        sum = sum + i;
        i = i + 1;
    } while (i < 10);
    return sum;
}

// run: test_break_do_while_loop_early() == 10

int test_break_do_while_loop_condition() {
    int sum = 0;
    int i = 0;
    do {
        sum = sum + i;
        if (sum >= 10) {
            break;
        }
        i = i + 1;
    } while (i < 10);
    return sum;
}

// run: test_break_do_while_loop_condition() == 10

int test_break_do_while_loop_after_first() {
    int sum = 0;
    int i = 0;
    do {
        sum = sum + i;
        i = i + 1;
        if (i >= 2) {
            break;
        }
    } while (i < 10);
    return sum;
}

// run: test_break_do_while_loop_after_first() == 1

int test_break_do_while_loop_nested_break() {
    int sum = 0;
    int i = 0;
    do {
        int j = 0;
        do {
            if (j >= 2) {
                break;
            }
            sum = sum + 1;
            j = j + 1;
        } while (j < 5);
        i = i + 1;
    } while (i < 5);
    return sum;
}

// run: test_break_do_while_loop_nested_break() == 10

