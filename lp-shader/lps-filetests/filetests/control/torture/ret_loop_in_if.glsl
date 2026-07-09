// test run

// ============================================================================
// Control-flow torture: early return from a loop nested inside a branch
// for-in-then and do-while-in-else with a conditional return inside.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ret_loop_in_if(int p, int q) {
    int t = 0;
    if (p > 0) {
        for (int i = 0; i < 3; i++) {
            if (i == q) {
                return t;
            }
            t = t * 10 + 1;
        }
    } else {
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_ret_loop_in_if(0, 0) == 23
// run: test_ret_loop_in_if(0, 2) == 23
// run: test_ret_loop_in_if(0, 3) == 23
// run: test_ret_loop_in_if(1, 0) == 0
// run: test_ret_loop_in_if(1, 2) == 11
// run: test_ret_loop_in_if(1, 3) == 1113

int test_ret_dowhile_in_else(int p, int q) {
    int t = 0;
    if (p > 0) {
        t = t * 10 + 2;
    } else {
        int i = 0;
        do {
            if (i == q) {
                return t;
            }
            t = t * 10 + 1;
            i = i + 1;
        } while (i < 3);
    }
    t = t * 10 + 3;
    return t;
}

// run: test_ret_dowhile_in_else(0, 0) == 0
// run: test_ret_dowhile_in_else(0, 2) == 11
// run: test_ret_dowhile_in_else(0, 3) == 1113
// run: test_ret_dowhile_in_else(1, 0) == 23
// run: test_ret_dowhile_in_else(1, 2) == 23
// run: test_ret_dowhile_in_else(1, 3) == 23
