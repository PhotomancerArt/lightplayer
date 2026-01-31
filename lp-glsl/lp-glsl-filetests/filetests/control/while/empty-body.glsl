// test run
// target riscv32.q32

// ============================================================================
// While loops with empty bodies
// ============================================================================

int test_while_loop_empty_body() {
    int i = 0;
    while (i < 5) {
        i = i + 1;
        // Body has increment, but testing loop structure
    }
    return i;
}

// run: test_while_loop_empty_body() == 5

int test_while_loop_condition_only() {
    int i = 0;
    while (i < 3) {
        i = i + 1;
    }
    return i;
}

// run: test_while_loop_condition_only() == 3

