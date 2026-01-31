// test run
// target riscv32.q32

// ============================================================================
// Complex condition expressions in while loops
// ============================================================================

int test_while_loop_complex_condition_and() {
    int sum = 0;
    int i = 0;
    while (i < 5 && i < 3) {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_while_loop_complex_condition_and() == 3

int test_while_loop_complex_condition_or() {
    int sum = 0;
    int i = 0;
    while (i < 2 || i < 5) {
        sum = sum + i;
        i = i + 1;
        if (i >= 4) break; // Prevent infinite loop
    }
    return sum;
}

// run: test_while_loop_complex_condition_or() == 6

int test_while_loop_complex_condition_equality() {
    int sum = 0;
    int i = 0;
    while (i != 5) {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_while_loop_complex_condition_equality() == 10

int test_while_loop_complex_condition_compound() {
    int sum = 0;
    int i = 1;
    while (i > 0 && i < 4) {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_while_loop_complex_condition_compound() == 6

