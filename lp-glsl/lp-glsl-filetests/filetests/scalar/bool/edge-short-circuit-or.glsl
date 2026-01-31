// test run
// target riscv32.q32

// ============================================================================
// Edge cases: Short-circuit evaluation for || operator
// Spec: || will only evaluate the right hand operand if the left hand operand evaluated to false
// ============================================================================

int test_short_circuit_or_true_left() {
    int x = 0;
    bool a = true;
    bool b = false;
    // If short-circuit works, b should not be evaluated when a is true
    // true || anything = true
    bool result = a || b;
    if (result == true) {
        x = 15;  // If short-circuit works correctly, result should be true
    }
    return x;
}

// run: test_short_circuit_or_true_left() == 15

int test_short_circuit_or_false_left() {
    int x = 0;
    bool a = false;
    bool b = true;
    // When left is false, right must be evaluated
    bool result = a || b;
    if (result == true) {
        x = 25;
    }
    return x;
}

// run: test_short_circuit_or_false_left() == 25

bool test_short_circuit_or_true_prevents_evaluation() {
    bool a = true;
    bool b = false;
    // true || anything should be true without evaluating right side
    return a || b;
}

// run: test_short_circuit_or_true_prevents_evaluation() == true

bool test_short_circuit_or_false_evaluates_both() {
    bool a = false;
    bool b = true;
    // false || true should evaluate both and return true
    return a || b;
}

// run: test_short_circuit_or_false_evaluates_both() == true

bool test_short_circuit_or_nested() {
    bool a = true;
    bool b = false;
    bool c = false;
    // (true || b) should short-circuit, so c should not matter
    // But actually, we evaluate (a || b) first, which short-circuits
    // Then we have true || c, which also short-circuits
    return (a || b) || c;
}

// run: test_short_circuit_or_nested() == true

bool test_short_circuit_or_chain() {
    bool a = false;
    bool b = false;
    bool c = true;
    // a || b evaluates to false, then false || c evaluates to true
    // c must be evaluated because left side was false
    return a || b || c;
}

// run: test_short_circuit_or_chain() == true

bool test_short_circuit_or_early_true() {
    bool a = true;
    bool b = false;
    bool c = false;
    bool d = false;
    // a is true, so short-circuit: b, c, d should not be evaluated
    return a || b || c || d;
}

// run: test_short_circuit_or_early_true() == true

bool test_short_circuit_or_all_false() {
    bool a = false;
    bool b = false;
    bool c = false;
    // All must be evaluated since all are false
    return a || b || c;
}

// run: test_short_circuit_or_all_false() == false

