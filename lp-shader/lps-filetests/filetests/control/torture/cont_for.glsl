// test run

// ============================================================================
// Control-flow torture: continue in for loops
// continue guarded in then arm / else arm at depth 1; continue in the
// inner loop of a nested pair (must re-test the inner condition only).
// For while/do-while the induction increment precedes the continue,
// exercising the continue-to-condition edge.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_cont_for_d1(int p) {
    int t = 0;
    for (int i = 0; i < 3; i++) {
        t = t * 10 + 1;
        if (i == p) {
            continue;
        }
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_cont_for_d1(0) == 112123
// run: test_cont_for_d1(1) == 121123
// run: test_cont_for_d1(2) == 121213
// run: test_cont_for_d1(3) == 1212123

int test_cont_for_d1_else(int p) {
    int t = 0;
    for (int i = 0; i < 3; i++) {
        if (i == p) {
            t = t * 10 + 1;
        } else {
            continue;
        }
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_cont_for_d1_else(0) == 123
// run: test_cont_for_d1_else(1) == 123
// run: test_cont_for_d1_else(2) == 123
// run: test_cont_for_d1_else(3) == 3

int test_cont_for_d2_inner(int p) {
    int t = 0;
    for (int i = 0; i < 2; i++) {
        t = t * 10 + 1;
        for (int j = 0; j < 2; j++) {
            if (j == p) {
                continue;
            }
            t = t * 10 + 2;
        }
        t = t * 10 + 3;
    }
    t = t * 10 + 4;
    return t;
}

// run: test_cont_for_d2_inner(0) == 1231234
// run: test_cont_for_d2_inner(1) == 1231234
// run: test_cont_for_d2_inner(2) == 122312234
// run: test_cont_for_d2_inner(3) == 122312234
