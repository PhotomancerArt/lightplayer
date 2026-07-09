// test run

// ============================================================================
// Control-flow torture: early return from nested if/else arms
// Returns from a depth-2 then arm, a depth-2 else arm (distinct
// constant), and a depth-3 leaf; fall-through paths must still run
// the post-if sites.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ret_ifnest_d2(int a, int b) {
    int t = 0;
    if (a > 0) {
        t = t * 10 + 1;
        if (b > 0) {
            return t;
        } else {
            t = t * 10 + 2;
        }
        t = t * 10 + 3;
    } else {
        if (b > 0) {
            t = t * 10 + 4;
        } else {
            return -1;
        }
    }
    t = t * 10 + 5;
    return t;
}

// run: test_ret_ifnest_d2(0, 0) == -1
// run: test_ret_ifnest_d2(0, 1) == 45
// run: test_ret_ifnest_d2(1, 0) == 1235
// run: test_ret_ifnest_d2(1, 1) == 1

int test_ret_ifnest_d3(int a, int b, int c) {
    int t = 0;
    if (a > 0) {
        t = t * 10 + 1;
        if (b > 0) {
            if (c > 0) {
                return t;
            } else {
                t = t * 10 + 2;
            }
        } else {
            t = t * 10 + 3;
        }
        t = t * 10 + 4;
    }
    t = t * 10 + 5;
    return t;
}

// run: test_ret_ifnest_d3(0, 0, 0) == 5
// run: test_ret_ifnest_d3(0, 0, 1) == 5
// run: test_ret_ifnest_d3(0, 1, 0) == 5
// run: test_ret_ifnest_d3(0, 1, 1) == 5
// run: test_ret_ifnest_d3(1, 0, 0) == 1345
// run: test_ret_ifnest_d3(1, 0, 1) == 1345
// run: test_ret_ifnest_d3(1, 1, 0) == 1245
// run: test_ret_ifnest_d3(1, 1, 1) == 1
