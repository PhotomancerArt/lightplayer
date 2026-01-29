// test run
// target riscv32.q32

// ============================================================================
// Break and continue edge cases for for loops
// ============================================================================

int test_break_in_if_in_loop() {
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        if (i >= 5) {
            break;
        }
        sum = sum + i;
    }
    return sum;
}

// run: test_break_in_if_in_loop() == 10

int test_continue_in_if_in_loop() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        if (i == 2) {
            continue;
        }
        sum = sum + i;
    }
    return sum;
}

// run: test_continue_in_if_in_loop() == 8

int test_break_nested_breaks_inner() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 5; j++) {
            if (j >= 2) {
                break; // Breaks inner loop only
            }
            sum = sum + 1;
        }
    }
    return sum;
}

// run: test_break_nested_breaks_inner() == 6

int test_continue_nested_continues_inner() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 5; j++) {
            if (j == 2) {
                continue; // Continues inner loop only
            }
            sum = sum + 1;
        }
    }
    return sum;
}

// run: test_continue_nested_continues_inner() == 12

int test_break_after_continue() {
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        if (i % 2 == 0) {
            continue;
        }
        if (i >= 7) {
            break;
        }
        sum = sum + i;
    }
    return sum;
}

// run: test_break_after_continue() == 9

int test_continue_after_break_impossible() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        if (i >= 3) {
            break;
        }
        // This continue never executes after break
        continue;
        sum = sum + i;
    }
    return sum;
}

// run: test_continue_after_break_impossible() == 0

int test_multiple_continues() {
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        if (i % 3 == 0) {
            continue;
        }
        if (i % 2 == 0) {
            continue;
        }
        sum = sum + i;
    }
    return sum;
}

// run: test_multiple_continues() == 13
