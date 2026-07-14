// test run

// ============================================================================
// Control-flow torture: nested if/else
// Depth-2 full binary if/else tree: child if/else in BOTH arms.
// Trace: t = t * 10 + k at each site; the final digit is the
// merge-point site after the outermost if.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifnest_d2_both(int a, int b) {
    int t = 0;
    if (a > 0) {
        if (b > 0) {
            t = t * 10 + 1;
        } else {
            t = t * 10 + 2;
        }
    } else {
        if (b > 0) {
            t = t * 10 + 3;
        } else {
            t = t * 10 + 4;
        }
    }
    t = t * 10 + 5;
    return t;
}

// run: test_ifnest_d2_both(0, 0) == 45
// run: test_ifnest_d2_both(0, 1) == 35
// run: test_ifnest_d2_both(1, 0) == 25
// run: test_ifnest_d2_both(1, 1) == 15
