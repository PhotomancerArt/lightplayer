// test run
// target riscv32.q32

// ============================================================================
// Return without value (void functions)
// ============================================================================

int test_void_return() {
    int x = 10;
    return x;
}

// run: test_void_return() == 10

int test_return_after_statements() {
    int x = 5;
    int y = 10;
    return x + y;
}

// run: test_return_after_statements() == 15

int test_return_immediate() {
    return 100;
}

// run: test_return_immediate() == 100





