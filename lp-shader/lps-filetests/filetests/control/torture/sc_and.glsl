// test run

// ============================================================================
// Control-flow torture: short-circuit && with side-effecting right operand
// chk(k, v) appends digit k to g_trace and returns v > 0, so the
// result exposes exactly which operands were evaluated and in what
// order. Wrongly evaluating a skipped operand changes the value.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int g_trace = 0;

bool chk(int k, int v) {
    g_trace = g_trace * 10 + k;
    return v > 0;
}

int test_sc_and(int a, int b) {
    g_trace = 0;
    int r = (chk(1, a) && chk(2, b) ? 1 : 2);
    return g_trace * 10 + r;
}

// run: test_sc_and(0, 0) == 12
// run: test_sc_and(0, 1) == 12
// run: test_sc_and(1, 0) == 122
// run: test_sc_and(1, 1) == 121

int test_sc_and_chain3(int a, int b, int c) {
    g_trace = 0;
    int r = (chk(1, a) && chk(2, b) && chk(3, c) ? 1 : 2);
    return g_trace * 10 + r;
}

// run: test_sc_and_chain3(0, 0, 0) == 12
// run: test_sc_and_chain3(0, 0, 1) == 12
// run: test_sc_and_chain3(0, 1, 0) == 12
// run: test_sc_and_chain3(0, 1, 1) == 12
// run: test_sc_and_chain3(1, 0, 0) == 122
// run: test_sc_and_chain3(1, 0, 1) == 122
// run: test_sc_and_chain3(1, 1, 0) == 1232
// run: test_sc_and_chain3(1, 1, 1) == 1231
