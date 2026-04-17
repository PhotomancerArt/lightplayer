// compile-opt(inline.mode, never)

// test run
//
// Performance: live range interference patterns.
// The greedy allocator assigns registers without liveness analysis,
// so values with overlapping live ranges may conflict and cause excess spills.
// These patterns stress different interference scenarios.

// Helper to create dependencies
int op(int a, int b) {
    return a + b;
}

// Diamond pattern: values fork then join
// a branches to b,c; then d = b + c
// Live ranges: a (short), b and c overlap, d (short)
int test_diamond_pattern() {
    int a = 10;           // def a
    int b = op(a, 1);   // use a, def b; a dies
    int c = op(a, 2);   // use a again (bug if a spilled), def c
    int d = op(b, c);   // use b,c; def d
    return d;
}

// Long-linear chain: each value used once, next created
// Minimal interference - each value has one user
int test_linear_chain() {
    int a = 1;
    int b = op(a, 1);   // use a
    int c = op(b, 1);   // use b
    int d = op(c, 1);   // use c
    int e = op(d, 1);   // use d
    int f = op(e, 1);   // use e
    int g = op(f, 1);   // use f
    int h = op(g, 1);   // use g
    return h;
}

// Star pattern: one value used by many consumers
// The central value must stay live through all uses
int test_star_pattern() {
    int center = 100;
    int a = op(center, 1);  // use center
    int b = op(center, 2);  // use center again
    int c = op(center, 3);  // use center again
    int d = op(center, 4);  // use center again
    int e = op(center, 5);  // use center again
    return a + b + c + d + e;  // center finally dies
}

// run: test_diamond_pattern() == 23
// run: test_linear_chain() == 8
// run: test_star_pattern() == 515
