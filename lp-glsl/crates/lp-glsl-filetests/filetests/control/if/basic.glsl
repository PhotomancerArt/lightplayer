// test run
// target riscv32.q32

// ============================================================================
// Basic if statements: true and false conditions
// ============================================================================

int test_if_true() {
    int x = 0;
    if (true) {
        x = 10;
    }
    return x;
}

// run: test_if_true() == 10

int test_if_false() {
    int x = 5;
    if (false) {
        x = 10;
    }
    return x;
}

// run: test_if_false() == 5

int test_if_condition_true() {
    int x = 0;
    if (5 > 3) {
        x = 20;
    }
    return x;
}

// run: test_if_condition_true() == 20

int test_if_condition_false() {
    int x = 0;
    if (3 > 5) {
        x = 20;
    }
    return x;
}

// run: test_if_condition_false() == 0

int test_if_equality() {
    int x = 0;
    if (10 == 10) {
        x = 100;
    }
    return x;
}

// run: test_if_equality() == 100

int test_if_inequality() {
    int x = 0;
    if (10 != 5) {
        x = 50;
    }
    return x;
}

// run: test_if_inequality() == 50





