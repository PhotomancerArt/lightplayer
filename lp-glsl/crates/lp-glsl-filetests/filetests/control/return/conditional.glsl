// test run
// target riscv32.q32

// ============================================================================
// Return in conditional statements
// ============================================================================

int test_return_in_if() {
    if (true) {
        return 10;
    }
    return 20;
}

// run: test_return_in_if() == 10

int test_return_in_if_else() {
    if (false) {
        return 10;
    } else {
        return 20;
    }
}

// run: test_return_in_if_else() == 20

int test_return_in_if_else_if() {
    if (false) {
        return 10;
    } else if (true) {
        return 20;
    } else {
        return 30;
    }
}

// run: test_return_in_if_else_if() == 20

int test_return_in_nested_if() {
    if (true) {
        if (true) {
            return 100;
        }
        return 200;
    }
    return 300;
}

// run: test_return_in_nested_if() == 100

int test_return_in_loop_if() {
    for (int i = 0; i < 10; i++) {
        if (i >= 3) {
            return i;
        }
    }
    return 0;
}

// run: test_return_in_loop_if() == 3





