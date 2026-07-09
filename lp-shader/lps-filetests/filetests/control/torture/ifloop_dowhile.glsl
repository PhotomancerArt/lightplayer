// test run

// ============================================================================
// Control-flow torture: dowhile loop nested inside branch arms
// dowhile loop in the then arm, the else arm, and both arms.
// Loop bound comes from the branch-selecting parameter where possible.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifloop_dowhile_then(int p) {
    int t = 0;
    if (p > 0) {
        t = t * 10 + 1;
        int i = 0;
        do {
            t = t * 10 + 2;
            i = i + 1;
        } while (i < p);
        t = t * 10 + 3;
    } else {
        t = t * 10 + 4;
    }
    t = t * 10 + 5;
    return t;
}

// run: test_ifloop_dowhile_then(0) == 45
// run: test_ifloop_dowhile_then(1) == 1235
// run: test_ifloop_dowhile_then(2) == 12235
// run: test_ifloop_dowhile_then(3) == 122235

int test_ifloop_dowhile_else(int p) {
    int t = 0;
    if (p == 0) {
        t = t * 10 + 1;
    } else {
        t = t * 10 + 2;
        int i = 0;
        do {
            t = t * 10 + 3;
            i = i + 1;
        } while (i < p);
        t = t * 10 + 4;
    }
    t = t * 10 + 5;
    return t;
}

// run: test_ifloop_dowhile_else(0) == 15
// run: test_ifloop_dowhile_else(1) == 2345
// run: test_ifloop_dowhile_else(2) == 23345
// run: test_ifloop_dowhile_else(3) == 233345

int test_ifloop_dowhile_both(int p) {
    int t = 0;
    if (p > 0) {
        int i = 0;
        do {
            t = t * 10 + 1;
            i = i + 1;
        } while (i < 2);
    } else {
        int j = 0;
        do {
            t = t * 10 + 2;
            j = j + 1;
        } while (j < 3);
    }
    t = t * 10 + 3;
    return t;
}

// run: test_ifloop_dowhile_both(0) == 2223
// run: test_ifloop_dowhile_both(1) == 113
