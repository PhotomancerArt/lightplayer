// test run
// target riscv32.q32

// ============================================================================
// Return edge cases
// Spec: Return causes immediate exit of the current function
//       Function main can use return
// ============================================================================

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

int test_return_in_loop() {
    for (int i = 0; i < 10; i++) {
        if (i >= 5) {
            return i;
        }
    }
    return 0;
}

// run: test_return_in_loop() == 5

int test_return_in_nested_loops() {
    for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 3; j++) {
            if (i + j >= 4) {
                return i * 10 + j;
            }
        }
    }
    return 0;
}

// run: test_return_in_nested_loops() == 22

int test_return_after_break() {
    for (int i = 0; i < 10; i++) {
        if (i >= 3) {
            break;
        }
    }
    return 100;
}

// run: test_return_after_break() == 100

int test_return_after_continue() {
    for (int i = 0; i < 5; i++) {
        if (i == 2) {
            continue;
        }
    }
    return 200;
}

// run: test_return_after_continue() == 200

int test_return_multiple_paths() {
    if (true) {
        return 10;
    }
    if (false) {
        return 20;
    }
    return 30;
}

// run: test_return_multiple_paths() == 10

int test_return_in_if_else_both() {
    if (true) {
        return 50;
    } else {
        return 60;
    }
}

// run: test_return_in_if_else_both() == 50

int test_return_in_if_else_false() {
    if (false) {
        return 50;
    } else {
        return 60;
    }
}

// run: test_return_in_if_else_false() == 60





