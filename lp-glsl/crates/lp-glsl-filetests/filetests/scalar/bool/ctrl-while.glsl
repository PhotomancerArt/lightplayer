// test run
// target riscv32.q32

// ============================================================================
// Control flow: while (bool) - conditions must evaluate to bool
// ============================================================================

int test_while_bool_true() {
    bool b = true;
    int x = 0;
    int count = 0;
    while (b && count < 3) {
        x = x + 1;
        count = count + 1;
    }
    return x;
}

// run: test_while_bool_true() == 3

int test_while_bool_false() {
    bool b = false;
    int x = 0;
    while (b) {
        x = x + 1;
    }
    return x;
}

// run: test_while_bool_false() == 0

int test_while_bool_condition_change() {
    bool b = true;
    int x = 0;
    int count = 0;
    while (b) {
        x = x + 1;
        count = count + 1;
        if (count >= 5) {
            b = false;  // Change condition to exit loop
        }
    }
    return x;
}

// run: test_while_bool_condition_change() == 5

int test_while_bool_expression() {
    int x = 0;
    int i = 0;
    while (i < 4) {  // Comparison returns bool
        x = x + 1;
        i = i + 1;
    }
    return x;
}

// run: test_while_bool_expression() == 4

int test_while_bool_not() {
    int y = 0;
    int c = 0;
    bool flag = false;
    while (!flag && c < 3) {
        y = y + 1;
        c = c + 1;
    }
    return y;
}

// run: test_while_bool_not() == 3

int test_while_bool_nested() {
    bool outer = true;
    bool inner = true;
    int x = 0;
    int outer_count = 0;
    while (outer && outer_count < 2) {
        int inner_count = 0;
        while (inner && inner_count < 2) {
            x = x + 1;
            inner_count = inner_count + 1;
        }
        inner = false;  // Prevent inner loop on second iteration
        outer_count = outer_count + 1;
    }
    return x;
}

// run: test_while_bool_nested() == 2

