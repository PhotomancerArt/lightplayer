// test run

// ============================================================================
// Control-flow torture: break and continue in the same loop body
// continue at i == p then break at i == q in one body (for and while).
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_brkcont_same_for(int p, int q) {
    int t = 0;
    for (int i = 0; i < 4; i++) {
        if (i == p) {
            continue;
        }
        t = t * 10 + 1;
        if (i == q) {
            break;
        }
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_brkcont_same_for(0, 1) == 13
// run: test_brkcont_same_for(0, 3) == 121213
// run: test_brkcont_same_for(0, 5) == 1212123
// run: test_brkcont_same_for(2, 1) == 1213
// run: test_brkcont_same_for(2, 3) == 121213
// run: test_brkcont_same_for(2, 5) == 1212123
// run: test_brkcont_same_for(5, 1) == 1213
// run: test_brkcont_same_for(5, 3) == 12121213
// run: test_brkcont_same_for(5, 5) == 121212123

int test_brkcont_same_while(int p, int q) {
    int t = 0;
    int i = 0;
    while (i < 4) {
        i = i + 1;
        if (i == p) {
            continue;
        }
        t = t * 10 + 1;
        if (i == q) {
            break;
        }
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_brkcont_same_while(1, 2) == 13
// run: test_brkcont_same_while(1, 4) == 121213
// run: test_brkcont_same_while(1, 5) == 1212123
// run: test_brkcont_same_while(3, 2) == 1213
// run: test_brkcont_same_while(3, 4) == 121213
// run: test_brkcont_same_while(3, 5) == 1212123
// run: test_brkcont_same_while(5, 2) == 1213
// run: test_brkcont_same_while(5, 4) == 12121213
// run: test_brkcont_same_while(5, 5) == 121212123
