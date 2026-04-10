// test run
//
// Performance: outgoing stack argument store overhead.
// When calling with >8 args (or >7 for sret callee), overflow args
// need sw to the caller's outgoing stack area before the call.

int callee_eight(int a, int b, int c, int d, int e, int f, int g, int h) {
    return a + b + c + d + e + f + g + h;
}

int callee_twelve(int a, int b, int c, int d, int e, int f, int g, int h, int i, int j, int k, int l) {
    return a + b + c + d + e + f + g + h + i + j + k + l;
}

// Baseline: 8 args (all in registers, no stack stores)
int test_call_eight_regs() {
    return callee_eight(1, 2, 3, 4, 5, 6, 7, 8);
}

// Stress: 12 args (4x sw to outgoing stack area)
int test_call_twelve_stack() {
    return callee_twelve(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
}

// run: test_call_eight_regs() == 36
// run: test_call_twelve_stack() == 78
