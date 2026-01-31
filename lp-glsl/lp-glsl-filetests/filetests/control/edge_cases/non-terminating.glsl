// test run
// target riscv32.q32

// ============================================================================
// Non-terminating loops (if supported)
// Spec: Non-terminating loops are allowed. The consequences of very long or
//       non-terminating loops are platform dependent.
// ============================================================================

int test_infinite_loop_with_break() {
    int sum = 0;
    int i = 0;
    while (true) {
        sum = sum + i;
        i = i + 1;
        if (i >= 5) {
            break;
        }
    }
    return sum;
}

// run: test_infinite_loop_with_break() == 10

int test_infinite_for_loop_with_break() {
    int sum = 0;
    for (int i = 0; ; i++) {
        sum = sum + i;
        if (i >= 4) {
            break;
        }
    }
    return sum;
}

// run: test_infinite_for_loop_with_break() == 10

int test_infinite_do_while_with_break() {
    int sum = 0;
    int i = 0;
    do {
        sum = sum + i;
        i = i + 1;
        if (i >= 5) {
            break;
        }
    } while (true);
    return sum;
}

// run: test_infinite_do_while_with_break() == 10

int test_loop_with_always_true_condition() {
    int sum = 0;
    int i = 0;
    while (1 == 1) {
        sum = sum + i;
        i = i + 1;
        if (i >= 4) {
            break;
        }
    }
    return sum;
}

// run: test_loop_with_always_true_condition() == 6





