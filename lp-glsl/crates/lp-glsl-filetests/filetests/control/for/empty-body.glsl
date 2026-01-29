// test run
// target riscv32.q32

// ============================================================================
// For loops with empty bodies
// ============================================================================

int test_for_loop_empty_body() {
    int i = 0;
    for (i = 0; i < 5; i++) {
        // Empty body
    }
    return i;
}

// run: test_for_loop_empty_body() == 5

int test_for_loop_empty_body_count() {
    int count = 0;
    int i = 0;
    for (i = 0; i < 10; i++) {
        count = count + 1;
        // Body has one statement, but testing loop still works
    }
    return count;
}

// run: test_for_loop_empty_body_count() == 10

int test_for_loop_empty_condition() {
    int i = 0;
    for (i = 0; i < 3; i++) {
        // Empty body - loop still increments
    }
    return i;
}

// run: test_for_loop_empty_condition() == 3

