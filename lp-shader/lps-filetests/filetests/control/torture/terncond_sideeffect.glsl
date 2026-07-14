// test run

// ============================================================================
// Control-flow torture: side-effecting ternary as an if condition
// if ((chk(a) ? chk(b) : chk(c))): exactly two chk calls run per
// test and the trace exposes which.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int g_trace = 0;

bool chk(int k, int v) {
    g_trace = g_trace * 10 + k;
    return v > 0;
}

int test_terncond_sideeffect(int a, int b, int c) {
    int t = 0;
    g_trace = 0;
    if ((chk(1, a) ? chk(2, b) : chk(3, c))) {
        t = 7;
    } else {
        t = 8;
    }
    return g_trace * 10 + t;
}

// run: test_terncond_sideeffect(0, 0, 0) == 138
// run: test_terncond_sideeffect(0, 0, 1) == 137
// run: test_terncond_sideeffect(0, 1, 0) == 138
// run: test_terncond_sideeffect(0, 1, 1) == 137
// run: test_terncond_sideeffect(1, 0, 0) == 128
// run: test_terncond_sideeffect(1, 0, 1) == 128
// run: test_terncond_sideeffect(1, 1, 0) == 127
// run: test_terncond_sideeffect(1, 1, 1) == 127
