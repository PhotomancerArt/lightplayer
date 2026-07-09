// test run

// ============================================================================
// Control-flow torture: loop-in-branch-in-loop mixes (for/while)
// for{if{while}else{..}} and while{if{for}..} shapes.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_mix_for_if_while(int p) {
    int t = 0;
    for (int i = 0; i < 2; i++) {
        if (i < p) {
            int j = 0;
            while (j < 2) {
                t = t * 10 + 1;
                j = j + 1;
            }
        } else {
            t = t * 10 + 2;
        }
    }
    t = t * 10 + 3;
    return t;
}

// run: test_mix_for_if_while(0) == 223
// run: test_mix_for_if_while(1) == 1123
// run: test_mix_for_if_while(2) == 11113

int test_mix_while_if_for(int p) {
    int t = 0;
    int i = 0;
    while (i < 3) {
        if (i == p) {
            for (int j = 0; j < 2; j++) {
                t = t * 10 + 1;
            }
        } else {
            t = t * 10 + 2;
        }
        i = i + 1;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_mix_while_if_for(0) == 11223
// run: test_mix_while_if_for(1) == 21123
// run: test_mix_while_if_for(2) == 22113
// run: test_mix_while_if_for(3) == 2223
