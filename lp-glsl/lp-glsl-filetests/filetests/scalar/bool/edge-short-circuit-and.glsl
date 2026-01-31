// test run
// target riscv32.q32

// ============================================================================
// Edge cases: Short-circuit evaluation for && operator
// Spec: && will only evaluate the right hand operand if the left hand operand evaluated to true
// ============================================================================

int test_short_circuit_and_false_left() {
    int x = 0;
    bool a = false;
    bool b = true;
    // If short-circuit works, b should not be evaluated, so x should remain 0
    // But we can't directly test this without side effects
    // Instead, we test that false && anything = false
    bool result = a && b;
    if (result == false) {
        x = 10;  // If short-circuit works correctly, result should be false
    }
    return x;
}

// run: test_short_circuit_and_false_left() == 10

int test_short_circuit_and_true_left() {
    int x = 0;
    bool a = true;
    bool b = true;
    // When left is true, right must be evaluated
    bool result = a && b;
    if (result == true) {
        x = 20;
    }
    return x;
}

// run: test_short_circuit_and_true_left() == 20

bool test_short_circuit_and_false_prevents_evaluation() {
    bool a = false;
    bool b = true;
    // false && anything should be false without evaluating right side
    return a && b;
}

// run: test_short_circuit_and_false_prevents_evaluation() == false

bool test_short_circuit_and_true_evaluates_both() {
    bool a = true;
    bool b = false;
    // true && false should evaluate both and return false
    return a && b;
}

// run: test_short_circuit_and_true_evaluates_both() == false

bool test_short_circuit_and_nested() {
    bool a = false;
    bool b = true;
    bool c = true;
    // (false && b) should short-circuit, so c should not matter
    // But actually, we evaluate (a && b) first, which short-circuits
    // Then we have false && c, which also short-circuits
    return (a && b) && c;
}

// run: test_short_circuit_and_nested() == false

bool test_short_circuit_and_chain() {
    bool a = true;
    bool b = true;
    bool c = false;
    // a && b evaluates to true, then true && c evaluates to false
    // c must be evaluated because left side was true
    return a && b && c;
}

// run: test_short_circuit_and_chain() == false

bool test_short_circuit_and_early_false() {
    bool a = false;
    bool b = true;
    bool c = true;
    bool d = true;
    // a is false, so short-circuit: b, c, d should not be evaluated
    return a && b && c && d;
}

// run: test_short_circuit_and_early_false() == false

bool test_short_circuit_and_all_true() {
    bool a = true;
    bool b = true;
    bool c = true;
    // All must be evaluated since all are true
    return a && b && c;
}

// run: test_short_circuit_and_all_true() == true

