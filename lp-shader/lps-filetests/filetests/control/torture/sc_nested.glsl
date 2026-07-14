// test run

// ============================================================================
// Control-flow torture: (a && b) || (c && d) with side effects
// All 16 input combinations; each has a distinct evaluation trace.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int g_trace = 0;

bool chk(int k, int v) {
    g_trace = g_trace * 10 + k;
    return v > 0;
}

int test_sc_nested_groups(int a, int b, int c, int d) {
    g_trace = 0;
    int r = (chk(1, a) && chk(2, b) || chk(3, c) && chk(4, d) ? 1 : 2);
    return g_trace * 10 + r;
}

// run: test_sc_nested_groups(0, 0, 0, 0) == 132
// run: test_sc_nested_groups(0, 0, 0, 1) == 132
// run: test_sc_nested_groups(0, 0, 1, 0) == 1342
// run: test_sc_nested_groups(0, 0, 1, 1) == 1341
// run: test_sc_nested_groups(0, 1, 0, 0) == 132
// run: test_sc_nested_groups(0, 1, 0, 1) == 132
// run: test_sc_nested_groups(0, 1, 1, 0) == 1342
// run: test_sc_nested_groups(0, 1, 1, 1) == 1341
// run: test_sc_nested_groups(1, 0, 0, 0) == 1232
// run: test_sc_nested_groups(1, 0, 0, 1) == 1232
// run: test_sc_nested_groups(1, 0, 1, 0) == 12342
// run: test_sc_nested_groups(1, 0, 1, 1) == 12341
// run: test_sc_nested_groups(1, 1, 0, 0) == 121
// run: test_sc_nested_groups(1, 1, 0, 1) == 121
// run: test_sc_nested_groups(1, 1, 1, 0) == 121
// run: test_sc_nested_groups(1, 1, 1, 1) == 121
