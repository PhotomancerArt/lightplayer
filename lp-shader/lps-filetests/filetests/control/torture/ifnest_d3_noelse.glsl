// test run

// ============================================================================
// Control-flow torture: nested if without else, depth 3
// Depth-3 else-less if chains, plus an else-less chain nested
// inside an if/else's else arm.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifnest_d3_noelse(int a, int b, int c) {
    int t = 0;
    if (a > 0) {
        t = t * 10 + 1;
        if (b > 0) {
            t = t * 10 + 2;
            if (c > 0) {
                t = t * 10 + 3;
            }
        }
        t = t * 10 + 4;
    }
    t = t * 10 + 5;
    return t;
}

// run: test_ifnest_d3_noelse(0, 0, 0) == 5
// run: test_ifnest_d3_noelse(0, 0, 1) == 5
// run: test_ifnest_d3_noelse(0, 1, 0) == 5
// run: test_ifnest_d3_noelse(0, 1, 1) == 5
// run: test_ifnest_d3_noelse(1, 0, 0) == 145
// run: test_ifnest_d3_noelse(1, 0, 1) == 145
// run: test_ifnest_d3_noelse(1, 1, 0) == 1245
// run: test_ifnest_d3_noelse(1, 1, 1) == 12345

int test_ifnest_d3_noelse_in_else(int a, int b, int c) {
    int t = 0;
    if (a > 0) {
        t = t * 10 + 1;
    } else {
        t = t * 10 + 2;
        if (b > 0) {
            if (c > 0) {
                t = t * 10 + 3;
            }
            t = t * 10 + 4;
        }
        t = t * 10 + 5;
    }
    t = t * 10 + 6;
    return t;
}

// run: test_ifnest_d3_noelse_in_else(0, 0, 0) == 256
// run: test_ifnest_d3_noelse_in_else(0, 0, 1) == 256
// run: test_ifnest_d3_noelse_in_else(0, 1, 0) == 2456
// run: test_ifnest_d3_noelse_in_else(0, 1, 1) == 23456
// run: test_ifnest_d3_noelse_in_else(1, 0, 0) == 16
// run: test_ifnest_d3_noelse_in_else(1, 0, 1) == 16
// run: test_ifnest_d3_noelse_in_else(1, 1, 0) == 16
// run: test_ifnest_d3_noelse_in_else(1, 1, 1) == 16
