// test run
// target riscv32.q32

// ============================================================================
// Variable shadowing scenarios - comprehensive tests
// These test edge cases that may reveal compiler bugs
// ============================================================================

int test_variable_shadowing_if() {
    int x = 10;
    if (true) {
        int x = 20;
        // Inner x shadows outer x
    }
    return x;
}

// run: test_variable_shadowing_if() == 10

int test_variable_shadowing_for() {
    int i = 100;
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        sum = sum + i;
    }
    return i;
}

// run: test_variable_shadowing_for() == 100

int test_variable_shadowing_nested() {
    int x = 5;
    if (true) {
        int x = 10;
        if (true) {
            int x = 15;
            // Innermost x
        }
        // Middle x
    }
    return x;
}

// run: test_variable_shadowing_nested() == 5

int test_variable_shadowing_loop() {
    int i = 50;
    int sum = 0;
    for (int i = 0; i < 2; i++) {
        int i = 10;
        sum = sum + i;
    }
    return i;
}

// run: test_variable_shadowing_loop() == 50

int test_variable_shadowing_loop_body() {
    int x = 100;
    for (int i = 0; i < 2; i++) {
        int x = i * 10;
        // Inner x shadows outer x in loop body
    }
    return x;
}

// run: test_variable_shadowing_loop_body() == 100

int test_variable_shadowing_nested_loops() {
    int i = 200;
    int sum = 0;
    for (int i = 0; i < 2; i++) {
        for (int i = 0; i < 2; i++) {
            sum = sum + i;
        }
    }
    return i;
}

// run: test_variable_shadowing_nested_loops() == 200

int test_variable_shadowing_if_in_loop() {
    int x = 500;
    for (int i = 0; i < 2; i++) {
        if (true) {
            int x = i * 100;
            // Inner x shadows outer x
        }
    }
    return x;
}

// run: test_variable_shadowing_if_in_loop() == 500

int test_variable_shadowing_loop_in_if() {
    int i = 1000;
    if (true) {
        for (int i = 0; i < 2; i++) {
            // Loop i shadows outer i
        }
    }
    return i;
}

// run: test_variable_shadowing_loop_in_if() == 1000

int test_variable_shadowing_triple_nested() {
    int x = 1;
    if (true) {
        int x = 2;
        if (true) {
            int x = 3;
            if (true) {
                int x = 4;
                // Deepest x
            }
            // Third level x
        }
        // Second level x
    }
    return x;
}

// run: test_variable_shadowing_triple_nested() == 1

int test_variable_shadowing_while_loop() {
    int i = 500;
    int j = 0;
    while (j < 2) {
        int i = j * 100;
        j = j + 1;
    }
    return i;
}

// run: test_variable_shadowing_while_loop() == 500
