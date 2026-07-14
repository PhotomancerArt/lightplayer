// test run

// ============================================================================
// Control-flow torture: nested if/else
// Depth-3 if/else chain; nesting arms per level = ET (T = then arm, E = else arm).
// Trace: t = t * 10 + k at each site; the final digit is the
// merge-point site after the outermost if.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifnest_d3_et(int a, int b, int c) {
    int t = 0;
    if (a > 0) {
        t = t * 10 + 8;
    } else {
        t = t * 10 + 6;
        if (b > 0) {
            t = t * 10 + 3;
            if (c > 0) {
                t = t * 10 + 1;
            } else {
                t = t * 10 + 2;
            }
            t = t * 10 + 4;
        } else {
            t = t * 10 + 5;
        }
        t = t * 10 + 7;
    }
    t = t * 10 + 9;
    return t;
}

// run: test_ifnest_d3_et(0, 0, 0) == 6579
// run: test_ifnest_d3_et(0, 0, 1) == 6579
// run: test_ifnest_d3_et(0, 1, 0) == 632479
// run: test_ifnest_d3_et(0, 1, 1) == 631479
// run: test_ifnest_d3_et(1, 0, 0) == 89
// run: test_ifnest_d3_et(1, 0, 1) == 89
// run: test_ifnest_d3_et(1, 1, 0) == 89
// run: test_ifnest_d3_et(1, 1, 1) == 89
