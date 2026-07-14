// test run

// ============================================================================
// Control-flow torture: nested if/else
// Depth-2 if/else chain; inner if/else nested in the then arm (shape T).
// Trace: t = t * 10 + k at each site; the final digit is the
// merge-point site after the outermost if.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifnest_d2_t(int a, int b) {
    int t = 0;
    if (a > 0) {
        t = t * 10 + 3;
        if (b > 0) {
            t = t * 10 + 1;
        } else {
            t = t * 10 + 2;
        }
        t = t * 10 + 4;
    } else {
        t = t * 10 + 5;
    }
    t = t * 10 + 6;
    return t;
}

// run: test_ifnest_d2_t(0, 0) == 56
// run: test_ifnest_d2_t(0, 1) == 56
// run: test_ifnest_d2_t(1, 0) == 3246
// run: test_ifnest_d2_t(1, 1) == 3146
