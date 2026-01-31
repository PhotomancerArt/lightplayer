// test run
// target riscv32.q32

// ============================================================================
// Nested for loops
// ============================================================================

int test_for_loop_nested() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 2; j++) {
            sum = sum + 1;
        }
    }
    return sum;
}

// run: test_for_loop_nested() == 6

int test_for_loop_nested_accumulate() {
    int sum = 0;
    for (int i = 0; i < 2; i++) {
        for (int j = 0; j < 3; j++) {
            sum = sum + i + j;
        }
    }
    return sum;
    // i=0: j=0,1,2 -> 0+0, 0+1, 0+2 = 3
    // i=1: j=0,1,2 -> 1+0, 1+1, 1+2 = 6
    // Total: 3 + 6 = 9
}

// run: test_for_loop_nested_accumulate() == 9

int test_for_loop_nested_triple() {
    int count = 0;
    for (int i = 0; i < 2; i++) {
        for (int j = 0; j < 2; j++) {
            for (int k = 0; k < 2; k++) {
                count = count + 1;
            }
        }
    }
    return count;
}

// run: test_for_loop_nested_triple() == 8

int test_for_loop_nested_different_ranges() {
    int sum = 0;
    for (int i = 0; i < 2; i++) {
        for (int j = 0; j < 4; j++) {
            sum = sum + 1;
        }
    }
    return sum;
}

// run: test_for_loop_nested_different_ranges() == 8

