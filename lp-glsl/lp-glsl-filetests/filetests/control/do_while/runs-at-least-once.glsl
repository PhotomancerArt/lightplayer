// test run
// target riscv32.q32

// ============================================================================
// Verify do-while executes at least once
// ============================================================================

int test_do_while_runs_at_least_once() {
    int x = 0;
    do {
        x = 10;
    } while (false);
    return x;
}

// run: test_do_while_runs_at_least_once() == 10

int test_do_while_false_condition() {
    int count = 0;
    do {
        count = count + 1;
    } while (count < 0);
    return count;
}

// run: test_do_while_false_condition() == 1

int test_do_while_zero_iterations_impossible() {
    int sum = 0;
    int i = 0;
    do {
        sum = sum + 1;
        i = i + 1;
    } while (i < 0);
    return sum;
}

// run: test_do_while_zero_iterations_impossible() == 1

