// test run

// ============================================================================
// Control-flow torture: nested if/else
// Depth-3 full binary if/else tree: child if/else in BOTH arms (8 leaves).
// Trace: t = t * 10 + k at each site; the final digit is the
// merge-point site after the outermost if.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifnest_d3_both(int a, int b, int c) {
    int t = 0;
    if (a > 0) {
        if (b > 0) {
            if (c > 0) {
                t = t * 10 + 1;
            } else {
                t = t * 10 + 2;
            }
        } else {
            if (c > 0) {
                t = t * 10 + 3;
            } else {
                t = t * 10 + 4;
            }
        }
    } else {
        if (b > 0) {
            if (c > 0) {
                t = t * 10 + 5;
            } else {
                t = t * 10 + 6;
            }
        } else {
            if (c > 0) {
                t = t * 10 + 7;
            } else {
                t = t * 10 + 8;
            }
        }
    }
    t = t * 10 + 9;
    return t;
}

// run: test_ifnest_d3_both(0, 0, 0) == 89
// run: test_ifnest_d3_both(0, 0, 1) == 79
// run: test_ifnest_d3_both(0, 1, 0) == 69
// run: test_ifnest_d3_both(0, 1, 1) == 59
// run: test_ifnest_d3_both(1, 0, 0) == 49
// run: test_ifnest_d3_both(1, 0, 1) == 39
// run: test_ifnest_d3_both(1, 1, 0) == 29
// run: test_ifnest_d3_both(1, 1, 1) == 19
