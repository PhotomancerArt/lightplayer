// test run
// target riscv32.q32

// ============================================================================
// Nested do-while loops
// ============================================================================

int test_do_while_loop_nested() {
    int sum = 0;
    int i = 0;
    do {
        int j = 0;
        do {
            sum = sum + 1;
            j = j + 1;
        } while (j < 2);
        i = i + 1;
    } while (i < 3);
    return sum;
}

// run: test_do_while_loop_nested() == 6

int test_do_while_loop_nested_accumulate() {
    int sum = 0;
    int i = 0;
    do {
        int j = 0;
        do {
            sum = sum + i + j;
            j = j + 1;
        } while (j < 3);
        i = i + 1;
    } while (i < 2);
    return sum;
    // i=0: j=0,1,2 -> 0+0, 0+1, 0+2 = 3
    // i=1: j=0,1,2 -> 1+0, 1+1, 1+2 = 6
    // Total: 3 + 6 = 9
}

// run: test_do_while_loop_nested_accumulate() == 9

int test_do_while_loop_nested_triple() {
    int count = 0;
    int i = 0;
    do {
        int j = 0;
        do {
            int k = 0;
            do {
                count = count + 1;
                k = k + 1;
            } while (k < 2);
            j = j + 1;
        } while (j < 2);
        i = i + 1;
    } while (i < 2);
    return count;
}

// run: test_do_while_loop_nested_triple() == 8

