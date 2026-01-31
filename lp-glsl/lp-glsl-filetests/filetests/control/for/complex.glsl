// test run
// target riscv32.q32

// ============================================================================
// Complex nested combinations with for loops
// ============================================================================

int test_complex_nested_1() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        if (i % 2 == 0) {
            for (int j = 0; j < 2; j++) {
                sum = sum + i + j;
            }
        }
    }
    return sum;
    // i=0: j=0,1 -> 0+0, 0+1 = 1
    // i=2: j=0,1 -> 2+0, 2+1 = 5
    // Total: 1 + 5 = 6
}

// run: test_complex_nested_1() == 6

int test_complex_nested_3() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        if (i == 1) {
            continue;
        }
        for (int j = 0; j < 2; j++) {
            if (j == 0) {
                break;
            }
            sum = sum + 1;
        }
    }
    return sum;
    // i=0: j=0 breaks -> 0
    // i=1: continue -> 0
    // i=2: j=0 breaks -> 0
    // Total: 0
}

// run: test_complex_nested_3() == 0

int test_complex_nested_4() {
    int sum = 0;
    for (int i = 0; i < 4; i++) {
        if (i < 2) {
            for (int j = 0; j < 2; j++) {
                sum = sum + 1;
            }
        } else {
            sum = sum + 10;
        }
    }
    return sum;
    // i=0,1: 2 each = 4
    // i=2,3: 10 each = 20
    // Total: 24
}

// run: test_complex_nested_4() == 24




