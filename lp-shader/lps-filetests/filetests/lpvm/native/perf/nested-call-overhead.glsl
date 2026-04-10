// test run
//
// Performance: register pressure across nested/cascaded calls.
// Each call clobbers caller-saved registers. The backend saves live
// values before each call and restores after. Nested calls compound this.

// Leaf callees with different arg counts
int leaf0() { return 1; }
int leaf1(int a) { return a + 1; }
int leaf2(int a, int b) { return a + b + 1; }
int leaf3(int a, int b, int c) { return a + b + c; }

// Baseline: single call, minimal live state
int test_single_call() {
    return leaf0();
}

// Sequential calls: preservation between calls
// After first call, result is live; must be preserved across second call
int test_sequential_calls() {
    int a = leaf1(1);   // call 1
    int b = leaf1(2);   // call 2 (must preserve a)
    int c = leaf1(3);   // call 3 (must preserve a,b)
    return a + b + c;
}

// Cascaded args: args to outer call are evaluated (inner calls)
// Each arg evaluation is a call that may clobber previous arg results
int test_cascaded_args() {
    // Evaluation order: leaf1(3), leaf1(2), leaf1(1)
    // Each inner call result must be preserved while evaluating next
    return leaf3(leaf1(1), leaf1(2), leaf1(3));
}

// Deep nesting: call chains
int test_deep_nesting() {
    return leaf1(leaf1(leaf1(leaf1(1))));
}

// Mixed: sequential + cascaded combination
int test_mixed_pattern() {
    int a = leaf1(1);
    int b = leaf2(a, leaf1(2));  // cascaded within sequential
    int c = leaf1(3);
    return leaf2(b, c);
}

// run: test_single_call() == 1
// run: test_sequential_calls() == 9
// run: test_cascaded_args() == 9
// run: test_deep_nesting() == 5
// run: test_mixed_pattern() == 11
