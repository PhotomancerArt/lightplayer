// test run
// target riscv32.q32

// ============================================================================
// Complex nested combinations with while loops
// ============================================================================

int test_complex_nested_2() {
    int sum = 0;
    int i = 0;
    while (i < 3) {
        if (i > 0) {
            int j = 0;
            while (j < 2) {
                sum = sum + i * j;
                j = j + 1;
            }
        }
        i = i + 1;
    }
    return sum;
    // i=1: j=0,1 -> 1*0, 1*1 = 1
    // i=2: j=0,1 -> 2*0, 2*1 = 2
    // Total: 1 + 2 = 3
}

// run: test_complex_nested_2() == 3




