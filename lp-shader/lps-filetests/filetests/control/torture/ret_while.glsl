// test run

// ============================================================================
// Control-flow torture: early return out of a while loop
// Return from the then arm mid-iteration and from an else arm;
// the post-loop site must not run on returning paths.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ret_while(int p) {
    int t = 0;
    int i = 0;
    while (i < 3) {
        t = t * 10 + 1;
        if (i == p) {
            return t;
        }
        t = t * 10 + 2;
        i = i + 1;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_ret_while(0) == 1
// run: test_ret_while(1) == 121
// run: test_ret_while(2) == 12121
// run: test_ret_while(3) == 1212123

int test_ret_while_else(int p) {
    int t = 0;
    int i = 0;
    while (i < 3) {
        if (i != p) {
            t = t * 10 + 1;
        } else {
            return t;
        }
        i = i + 1;
    }
    t = t * 10 + 2;
    return t;
}

// run: test_ret_while_else(0) == 0
// run: test_ret_while_else(1) == 1
// run: test_ret_while_else(2) == 11
// run: test_ret_while_else(3) == 1112
