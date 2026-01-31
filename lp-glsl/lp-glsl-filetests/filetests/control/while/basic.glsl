// test run
// target riscv32.q32

// ============================================================================
// Basic while loops
// ============================================================================

int test_while_loop_basic() {
    int i = 0;
    int sum = 0;
    while (i < 5) {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_while_loop_basic() == 10

int test_while_loop_count() {
    int count = 0;
    int i = 0;
    while (i < 10) {
        count = count + 1;
        i = i + 1;
    }
    return count;
}

// run: test_while_loop_count() == 10

int test_while_loop_zero_iterations() {
    int sum = 0;
    int i = 0;
    while (i < 0) {
        sum = sum + 1;
        i = i + 1;
    }
    return sum;
}

// run: test_while_loop_zero_iterations() == 0

int test_while_loop_single_iteration() {
    int sum = 0;
    int i = 0;
    while (i < 1) {
        sum = sum + 1;
        i = i + 1;
    }
    return sum;
}

// run: test_while_loop_single_iteration() == 1

int test_while_loop_decrement() {
    int sum = 0;
    int i = 5;
    while (i > 0) {
        sum = sum + i;
        i = i - 1;
    }
    return sum;
}

// run: test_while_loop_decrement() == 15

