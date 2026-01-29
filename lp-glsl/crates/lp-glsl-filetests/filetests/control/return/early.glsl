// test run
// target riscv32.q32

// ============================================================================
// Early returns in functions
// ============================================================================

int test_early_return_condition_true() {
    if (true) {
        return 10;
    }
    return 20;
}

// run: test_early_return_condition_true() == 10

int test_early_return_condition_false() {
    if (false) {
        return 10;
    }
    return 20;
}

// run: test_early_return_condition_false() == 20

int test_early_return_in_loop() {
    for (int i = 0; i < 10; i++) {
        if (i >= 5) {
            return i;
        }
    }
    return 100;
}

// run: test_early_return_in_loop() == 5

int test_early_return_nested() {
    if (true) {
        if (true) {
            return 50;
        }
        return 40;
    }
    return 30;
}

// run: test_early_return_nested() == 50

int test_early_return_multiple() {
    if (false) {
        return 10;
    }
    if (true) {
        return 20;
    }
    return 30;
}

// run: test_early_return_multiple() == 20





