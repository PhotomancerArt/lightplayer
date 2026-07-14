// test run

// ============================================================================
// Control-flow torture: short-circuit operators as ternary conditions
// (chk && chk) ? 7 : 8 and the || variant.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int g_trace = 0;

bool chk(int k, int v) {
    g_trace = g_trace * 10 + k;
    return v > 0;
}

int test_sc_in_ternary(int a, int b) {
    g_trace = 0;
    int r = (chk(1, a) && chk(2, b) ? 7 : 8);
    return g_trace * 10 + r;
}

// run: test_sc_in_ternary(0, 0) == 18
// run: test_sc_in_ternary(0, 1) == 18
// run: test_sc_in_ternary(1, 0) == 128
// run: test_sc_in_ternary(1, 1) == 127

int test_sc_in_ternary_or(int a, int b) {
    g_trace = 0;
    int r = (chk(1, a) || chk(2, b) ? 7 : 8);
    return g_trace * 10 + r;
}

// run: test_sc_in_ternary_or(0, 0) == 128
// run: test_sc_in_ternary_or(0, 1) == 127
// run: test_sc_in_ternary_or(1, 0) == 17
// run: test_sc_in_ternary_or(1, 1) == 17
