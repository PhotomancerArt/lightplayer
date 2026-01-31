// test run
// target riscv32.q32

// ============================================================================
// Basic if-else statements
// ============================================================================

int test_if_else_true() {
    int x = 0;
    if (true) {
        x = 10;
    } else {
        x = 20;
    }
    return x;
}

// run: test_if_else_true() == 10

int test_if_else_false() {
    int x = 0;
    if (false) {
        x = 10;
    } else {
        x = 20;
    }
    return x;
}

// run: test_if_else_false() == 20

int test_if_else_condition_true() {
    int x = 0;
    if (5 > 3) {
        x = 100;
    } else {
        x = 200;
    }
    return x;
}

// run: test_if_else_condition_true() == 100

int test_if_else_condition_false() {
    int x = 0;
    if (3 > 5) {
        x = 100;
    } else {
        x = 200;
    }
    return x;
}

// run: test_if_else_condition_false() == 200

int test_if_else_equality() {
    int x = 0;
    if (10 == 10) {
        x = 50;
    } else {
        x = 100;
    }
    return x;
}

// run: test_if_else_equality() == 50





