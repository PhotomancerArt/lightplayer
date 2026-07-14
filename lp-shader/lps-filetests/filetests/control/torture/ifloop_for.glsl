// test run

// ============================================================================
// Control-flow torture: for loop nested inside branch arms
// for loop in the then arm, the else arm, and both arms.
// Loop bound comes from the branch-selecting parameter where possible.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifloop_for_then(int p) {
    int t = 0;
    if (p > 0) {
        t = t * 10 + 1;
        for (int i = 0; i < p; i++) {
            t = t * 10 + 2;
        }
        t = t * 10 + 3;
    } else {
        t = t * 10 + 4;
    }
    t = t * 10 + 5;
    return t;
}

// run: test_ifloop_for_then(0) == 45
// run: test_ifloop_for_then(1) == 1235
// run: test_ifloop_for_then(2) == 12235
// run: test_ifloop_for_then(3) == 122235

int test_ifloop_for_else(int p) {
    int t = 0;
    if (p == 0) {
        t = t * 10 + 1;
    } else {
        t = t * 10 + 2;
        for (int i = 0; i < p; i++) {
            t = t * 10 + 3;
        }
        t = t * 10 + 4;
    }
    t = t * 10 + 5;
    return t;
}

// run: test_ifloop_for_else(0) == 15
// run: test_ifloop_for_else(1) == 2345
// run: test_ifloop_for_else(2) == 23345
// run: test_ifloop_for_else(3) == 233345

int test_ifloop_for_both(int p) {
    int t = 0;
    if (p > 0) {
        for (int i = 0; i < 2; i++) {
            t = t * 10 + 1;
        }
    } else {
        for (int j = 0; j < 3; j++) {
            t = t * 10 + 2;
        }
    }
    t = t * 10 + 3;
    return t;
}

// run: test_ifloop_for_both(0) == 2223
// run: test_ifloop_for_both(1) == 113
