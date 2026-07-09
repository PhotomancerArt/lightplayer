// test run

// ============================================================================
// Control-flow torture: else-if chain inside for loop
// 3-iteration for loop; 3-way chain on i vs p.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_loopif_for_chain(int p) {
    int t = 0;
    for (int i = 0; i < 3; i++) {
        if (i < p) {
            t = t * 10 + 1;
        } else {
            if (i == p) {
                t = t * 10 + 2;
            } else {
                t = t * 10 + 3;
            }
        }
    }
    t = t * 10 + 4;
    return t;
}

// run: test_loopif_for_chain(0) == 2334
// run: test_loopif_for_chain(1) == 1234
// run: test_loopif_for_chain(2) == 1124
// run: test_loopif_for_chain(3) == 1114
