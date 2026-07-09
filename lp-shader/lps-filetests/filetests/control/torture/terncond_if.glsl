// test run

// ============================================================================
// Control-flow torture: ternary inside an if condition
// if ((p > 0 ? a : b) > 0) and a comparison with ternaries on both sides.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_terncond_if(int p, int a, int b) {
    int t = 0;
    if ((p > 0 ? a : b) > 0) {
        t = t * 10 + 1;
    } else {
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_terncond_if(0, 0, 0) == 23
// run: test_terncond_if(0, 0, 1) == 13
// run: test_terncond_if(0, 1, 0) == 23
// run: test_terncond_if(0, 1, 1) == 13
// run: test_terncond_if(1, 0, 0) == 23
// run: test_terncond_if(1, 0, 1) == 23
// run: test_terncond_if(1, 1, 0) == 13
// run: test_terncond_if(1, 1, 1) == 13

int test_terncond_if_both_sides(int p, int q) {
    int t = 0;
    if ((p > 0 ? 3 : 1) > (q > 0 ? 2 : 0)) {
        t = t * 10 + 1;
    } else {
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_terncond_if_both_sides(0, 0) == 13
// run: test_terncond_if_both_sides(0, 1) == 23
// run: test_terncond_if_both_sides(1, 0) == 13
// run: test_terncond_if_both_sides(1, 1) == 13
