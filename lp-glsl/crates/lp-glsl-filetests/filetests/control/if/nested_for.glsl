// test run
// target riscv32.q32

// ============================================================================
// For loops inside if statements
// ============================================================================

int test_for_loop_in_if() {
    int sum = 0;
    if (true) {
        for (int i = 0; i < 3; i++) {
            sum = sum + i;
        }
    }
    return sum;
}

// run: test_for_loop_in_if() == 3

int test_for_loop_in_if_else() {
    int sum = 0;
    if (false) {
        for (int i = 0; i < 2; i++) {
            sum = sum + i;
        }
    } else {
        for (int i = 0; i < 3; i++) {
            sum = sum + i;
        }
    }
    return sum;
}

// run: test_for_loop_in_if_else() == 3

int test_nested_loop_in_if() {
    int sum = 0;
    if (true) {
        for (int i = 0; i < 2; i++) {
            for (int j = 0; j < 2; j++) {
                sum = sum + 1;
            }
        }
    }
    return sum;
}

// run: test_nested_loop_in_if() == 4




