// test run

// ============================================================================
// Control-flow torture: early return out of a dowhile loop
// Return from the then arm mid-iteration and from an else arm;
// the post-loop site must not run on returning paths.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ret_dowhile(int p) {
    int t = 0;
    int i = 0;
    do {
        t = t * 10 + 1;
        if (i == p) {
            return t;
        }
        t = t * 10 + 2;
        i = i + 1;
    } while (i < 3);
    t = t * 10 + 3;
    return t;
}

// run: test_ret_dowhile(0) == 1
// run: test_ret_dowhile(1) == 121
// run: test_ret_dowhile(2) == 12121
// run: test_ret_dowhile(3) == 1212123

int test_ret_dowhile_else(int p) {
    int t = 0;
    int i = 0;
    do {
        if (i != p) {
            t = t * 10 + 1;
        } else {
            return t;
        }
        i = i + 1;
    } while (i < 3);
    t = t * 10 + 2;
    return t;
}

// run: test_ret_dowhile_else(0) == 0
// run: test_ret_dowhile_else(1) == 1
// run: test_ret_dowhile_else(2) == 11
// run: test_ret_dowhile_else(3) == 1112
