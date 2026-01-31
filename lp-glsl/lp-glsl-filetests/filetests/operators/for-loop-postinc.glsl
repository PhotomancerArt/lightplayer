// test run
// target riscv32.q32

int test_for_loop_postinc() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        sum = sum + i;
    }
    return sum;
}

// run: test_for_loop_postinc() == 10
