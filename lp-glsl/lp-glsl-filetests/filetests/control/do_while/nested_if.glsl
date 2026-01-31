// test run
// target riscv32.q32

// ============================================================================
// If statements inside do-while loops
// ============================================================================

int test_if_in_do_while_loop() {
    int sum = 0;
    int i = 0;
    do {
        if (i % 2 == 0) {
            sum = sum + i;
        }
        i = i + 1;
    } while (i < 5);
    return sum;
}

// run: test_if_in_do_while_loop() == 6

int test_if_else_in_do_while_loop() {
    int sum = 0;
    int i = 0;
    do {
        if (i % 2 == 0) {
            sum = sum + i;
        } else {
            sum = sum + i * 2;
        }
        i = i + 1;
    } while (i < 5);
    return sum;
}

// run: test_if_else_in_do_while_loop() == 14

int test_nested_if_in_do_while_loop() {
    int sum = 0;
    int i = 0;
    do {
        if (i > 1) {
            if (i < 4) {
                sum = sum + i;
            }
        }
        i = i + 1;
    } while (i < 5);
    return sum;
}

// run: test_nested_if_in_do_while_loop() == 5




