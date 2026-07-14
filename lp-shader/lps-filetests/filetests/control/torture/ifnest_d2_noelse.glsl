// test run

// ============================================================================
// Control-flow torture: nested if without else
// Depth-2 if-in-if where neither if has an else arm.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifnest_d2_noelse(int a, int b) {
    int t = 0;
    if (a > 0) {
        t = t * 10 + 1;
        if (b > 0) {
            t = t * 10 + 2;
        }
        t = t * 10 + 3;
    }
    t = t * 10 + 4;
    return t;
}

// run: test_ifnest_d2_noelse(0, 0) == 4
// run: test_ifnest_d2_noelse(0, 1) == 4
// run: test_ifnest_d2_noelse(1, 0) == 134
// run: test_ifnest_d2_noelse(1, 1) == 1234
