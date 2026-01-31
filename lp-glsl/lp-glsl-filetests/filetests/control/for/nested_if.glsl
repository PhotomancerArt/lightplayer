// test run
// target riscv32.q32

// ============================================================================
// If statements inside for loops
// ============================================================================

int test_if_in_for_loop() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        if (i % 2 == 0) {
            sum = sum + i;
        }
    }
    return sum;
}

// run: test_if_in_for_loop() == 6

int test_if_else_in_for_loop() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        if (i % 2 == 0) {
            sum = sum + i;
        } else {
            sum = sum + i * 2;
        }
    }
    return sum;
}

// run: test_if_else_in_for_loop() == 14

int test_nested_if_in_loop() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        if (i > 1) {
            if (i < 4) {
                sum = sum + i;
            }
        }
    }
    return sum;
}

// run: test_nested_if_in_loop() == 5




