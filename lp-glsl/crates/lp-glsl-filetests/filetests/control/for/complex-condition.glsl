// test run
// target riscv32.q32

// ============================================================================
// Complex condition expressions in for loops
// ============================================================================

int test_for_loop_complex_condition_and() {
    int sum = 0;
    for (int i = 0; i < 5 && i < 3; i++) {
        sum = sum + i;
    }
    return sum;
}

// run: test_for_loop_complex_condition_and() == 3

int test_for_loop_complex_condition_or() {
    int sum = 0;
    int i = 0;
    for (i = 0; i < 2 || i < 5; i++) {
        sum = sum + i;
        if (i >= 3) break; // Prevent infinite loop
    }
    return sum;
}

// run: test_for_loop_complex_condition_or() == 6

int test_for_loop_complex_condition_equality() {
    int sum = 0;
    for (int i = 0; i != 5; i++) {
        sum = sum + i;
    }
    return sum;
}

// run: test_for_loop_complex_condition_equality() == 10

int test_for_loop_complex_condition_compound() {
    int sum = 0;
    for (int i = 1; i > 0 && i < 4; i++) {
        sum = sum + i;
    }
    return sum;
}

// run: test_for_loop_complex_condition_compound() == 6

