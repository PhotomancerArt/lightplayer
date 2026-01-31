// test run
// target riscv32.q32

// ============================================================================
// While loops inside if statements
// ============================================================================

int test_while_loop_in_if() {
    int sum = 0;
    if (true) {
        int i = 0;
        while (i < 3) {
            sum = sum + i;
            i = i + 1;
        }
    }
    return sum;
}

// run: test_while_loop_in_if() == 3




