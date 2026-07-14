// test run

// ============================================================================
// Control-flow torture: loop-in-branch-in-loop mixes (do-while)
// dowhile{if{for}..} and for{if{dowhile}else{..}} shapes.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_mix_dowhile_if_for(int p) {
    int t = 0;
    int i = 0;
    do {
        if (i < p) {
            for (int j = 0; j < 2; j++) {
                t = t * 10 + 1;
            }
        } else {
            t = t * 10 + 2;
        }
        i = i + 1;
    } while (i < 3);
    t = t * 10 + 3;
    return t;
}

// run: test_mix_dowhile_if_for(0) == 2223
// run: test_mix_dowhile_if_for(1) == 11223
// run: test_mix_dowhile_if_for(2) == 111123
// run: test_mix_dowhile_if_for(3) == 1111113

int test_mix_for_if_dowhile(int p) {
    int t = 0;
    for (int i = 0; i < 2; i++) {
        if (i == p) {
            int j = 0;
            do {
                t = t * 10 + 1;
                j = j + 1;
            } while (j < 2);
        } else {
            t = t * 10 + 2;
        }
    }
    t = t * 10 + 3;
    return t;
}

// run: test_mix_for_if_dowhile(0) == 1123
// run: test_mix_for_if_dowhile(1) == 2113
// run: test_mix_for_if_dowhile(2) == 223
