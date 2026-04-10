// test run
//
// Performance: caller-saved register preservation across calls.
// The native backend spills caller-saved regs that are live across calls
// to the spill area, then reloads after. This measures that overhead.

int callee_identity(int x) {
    return x;
}

// Baseline: no live values across call (no preservation needed)
int test_no_preserve_across_call() {
    int r = callee_identity(42);
    return r;
}

// Stress: 4 values live across call (4x sw before, 4x lw after)
int test_four_live_across_call() {
    int a = 1;
    int b = 2;
    int c = 3;
    int d = 4;
    int r = callee_identity(42);  // a,b,c,d must be preserved
    return a + b + c + d + r;
}

// Stress: 8 values live across call (heavy spill/reload)
int test_eight_live_across_call() {
    int a = 1;
    int b = 2;
    int c = 3;
    int d = 4;
    int e = 5;
    int f = 6;
    int g = 7;
    int h = 8;
    int r = callee_identity(42);  // a-h must be preserved
    return a + b + c + d + e + f + g + h + r;
}

// run: test_no_preserve_across_call() == 42
// run: test_four_live_across_call() == 52
// run: test_eight_live_across_call() == 78
