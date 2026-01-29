// test run
// target riscv32.q32

// ============================================================================
// Short-circuit evaluation tests
// Spec: Only one of the second and third expressions is evaluated
//       If condition is true, only exp2 is evaluated
//       If condition is false, only exp3 is evaluated
// ============================================================================

// Note: Directly testing short-circuit without side effects is difficult.
// We test the behavior by ensuring the correct branch value is returned.

int test_short_circuit_true_branch() {
    bool b = true;
    // When true, only first branch should be evaluated
    // We can't directly test non-evaluation, but we verify correct result
    return b ? 10 : 20;
}

// run: test_short_circuit_true_branch() == 10

int test_short_circuit_false_branch() {
    bool b = false;
    // When false, only second branch should be evaluated
    return b ? 10 : 20;
}

// run: test_short_circuit_false_branch() == 20

int test_short_circuit_nested_true() {
    bool a = true;
    bool b = false;
    // Outer: true -> evaluate first branch
    // Inner: false -> evaluate second branch of inner ternary
    return a ? (b ? 1 : 2) : (b ? 3 : 4);
}

// run: test_short_circuit_nested_true() == 2

int test_short_circuit_nested_false() {
    bool a = false;
    bool b = true;
    // Outer: false -> evaluate second branch
    // Inner: true -> evaluate first branch of inner ternary
    return a ? (b ? 1 : 2) : (b ? 3 : 4);
}

// run: test_short_circuit_nested_false() == 3

int test_short_circuit_deeply_nested() {
    bool a = true;
    bool b = true;
    bool c = false;
    // a=true -> first branch
    // b=true -> first branch of inner
    // c=false -> second branch of innermost
    return a ? (b ? (c ? 1 : 2) : 3) : 4;
}

// run: test_short_circuit_deeply_nested() == 2

int test_short_circuit_complex_condition() {
    bool a = true;
    bool b = false;
    // (a && b) = false, so evaluate second branch
    return (a && b) ? 100 : 200;
}

// run: test_short_circuit_complex_condition() == 200

int test_short_circuit_with_comparison() {
    int x = 5;
    int y = 3;
    // (x > y) = true, so evaluate first branch
    return (x > y) ? 30 : 40;
}

// run: test_short_circuit_with_comparison() == 30

int test_short_circuit_chain() {
    bool a = true;
    bool b = false;
    // Chain of ternaries, right-to-left associativity
    // a ? (b ? 1 : 2) : 3
    // a=true -> first branch: (b ? 1 : 2)
    // b=false -> second branch: 2
    return a ? b ? 1 : 2 : 3;
}

// run: test_short_circuit_chain() == 2





