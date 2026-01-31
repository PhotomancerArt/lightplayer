// test run
// target riscv32.q32

// ============================================================================
// Continue in nested for loops
// ============================================================================

int test_continue_nested_for_inner() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 5; j++) {
            if (j == 2) {
                continue;
            }
            sum = sum + 1;
        }
    }
    return sum;
}

// run: test_continue_nested_for_inner() == 12

int test_continue_nested_triple() {
    int count = 0;
    for (int i = 0; i < 2; i++) {
        for (int j = 0; j < 3; j++) {
            for (int k = 0; k < 4; k++) {
                if (k == 2) {
                    continue;
                }
                count = count + 1;
            }
        }
    }
    return count;
}

// run: test_continue_nested_triple() == 18

int test_continue_nested_mixed() {
    int sum = 0;
    for (int i = 0; i < 2; i++) {
        int j = 0;
        while (j < 4) {
            if (j == 1) {
                j = j + 1;
                continue;
            }
            sum = sum + 1;
            j = j + 1;
        }
    }
    return sum;
}

// run: test_continue_nested_mixed() == 6
