// test run
// target riscv32.q32

// ============================================================================
// For loops with decrement
// ============================================================================

int test_for_loop_decrement() {
    int sum = 0;
    for (int i = 5; i > 0; i--) {
        sum = sum + i;
    }
    return sum;
}

// run: test_for_loop_decrement() == 15

int test_for_loop_decrement_count() {
    int count = 0;
    for (int i = 10; i >= 1; i--) {
        count = count + 1;
    }
    return count;
}

// run: test_for_loop_decrement_count() == 10

int test_for_loop_decrement_range() {
    int sum = 0;
    for (int i = 10; i >= 5; i--) {
        sum = sum + i;
    }
    return sum;
}

// run: test_for_loop_decrement_range() == 45

int test_for_loop_decrement_zero() {
    int sum = 0;
    for (int i = 3; i >= 0; i--) {
        sum = sum + i;
    }
    return sum;
}

// run: test_for_loop_decrement_zero() == 6

