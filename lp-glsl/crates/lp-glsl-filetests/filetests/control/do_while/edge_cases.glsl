// test run
// target riscv32.q32

// ============================================================================
// Break and continue edge cases for do-while loops
// ============================================================================

int test_continue_in_do_while() {
    int sum = 0;
    int i = 0;
    do {
        if (i == 1) {
            i = i + 1;
            continue;
        }
        sum = sum + i;
        i = i + 1;
    } while (i < 4);
    return sum;
}

// run: test_continue_in_do_while() == 5

int test_break_in_do_while() {
    int sum = 0;
    int i = 0;
    do {
        if (i >= 3) {
            break;
        }
        sum = sum + i;
        i = i + 1;
    } while (i < 10);
    return sum;
}

// run: test_break_in_do_while() == 3
