// test run
// target riscv32.q32

// ============================================================================
// Condition expression edge cases
// Spec: Expressions for condition-expression must evaluate to a Boolean
//       Vector types are not accepted as the expression to if
// ============================================================================

int test_if_boolean_true() {
    bool b = true;
    int x = 0;
    if (b) {
        x = 10;
    }
    return x;
}

// run: test_if_boolean_true() == 10

int test_if_boolean_false() {
    bool b = false;
    int x = 0;
    if (b) {
        x = 10;
    }
    return x;
}

// run: test_if_boolean_false() == 0

int test_if_comparison_gt() {
    int x = 0;
    if (5 > 3) {
        x = 10;
    }
    return x;
}

// run: test_if_comparison_gt() == 10

int test_if_comparison_lt() {
    int x = 0;
    if (3 < 5) {
        x = 10;
    }
    return x;
}

// run: test_if_comparison_lt() == 10

int test_if_comparison_eq() {
    int x = 0;
    if (5 == 5) {
        x = 10;
    }
    return x;
}

// run: test_if_comparison_eq() == 10

int test_if_comparison_ne() {
    int x = 0;
    if (5 != 3) {
        x = 10;
    }
    return x;
}

// run: test_if_comparison_ne() == 10

int test_if_logical_and() {
    int x = 0;
    if (true && true) {
        x = 10;
    }
    return x;
}

// run: test_if_logical_and() == 10

int test_if_logical_or() {
    int x = 0;
    if (false || true) {
        x = 10;
    }
    return x;
}

// run: test_if_logical_or() == 10

int test_if_logical_not() {
    int x = 0;
    if (!false) {
        x = 10;
    }
    return x;
}

// run: test_if_logical_not() == 10

int test_loop_condition_boolean() {
    int sum = 0;
    bool b = true;
    int i = 0;
    while (b && i < 3) {
        sum = sum + i;
        i = i + 1;
        if (i >= 3) {
            b = false;
        }
    }
    return sum;
}

// run: test_loop_condition_boolean() == 3

int test_loop_condition_complex() {
    int sum = 0;
    int i = 0;
    while (i < 5 && i >= 0) {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_loop_condition_complex() == 10





