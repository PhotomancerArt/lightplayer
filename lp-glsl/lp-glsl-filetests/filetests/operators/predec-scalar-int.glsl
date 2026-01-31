// test run
// target riscv32.q32

int test_predec_scalar_int() {
    int x = 8;
    int result = --x;  // Should decrement x to 7, then return 7
    return result;     // Should return 7
}

// run: test_predec_scalar_int() == 7
