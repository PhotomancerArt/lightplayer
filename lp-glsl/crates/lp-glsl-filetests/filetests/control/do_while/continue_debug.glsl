// test run
// target riscv32.q32


int test_continue_do_while_loop_after_first() {
    int sum = 0;
    int i = 0;
    do {
        sum = sum + i;
        i = i + 1;
        if (i >= 2) {
            continue;
        }
    } while (i < 5);
    return sum;
}

// run: test_continue_do_while_loop_after_first() == 10

