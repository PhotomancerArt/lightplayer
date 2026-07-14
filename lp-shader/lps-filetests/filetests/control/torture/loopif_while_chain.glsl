// test run

// ============================================================================
// Control-flow torture: else-if chain inside while loop
// 3-iteration while loop; 3-way chain on i vs p.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_loopif_while_chain(int p) {
    int t = 0;
    int i = 0;
    while (i < 3) {
        if (i < p) {
            t = t * 10 + 1;
        } else {
            if (i == p) {
                t = t * 10 + 2;
            } else {
                t = t * 10 + 3;
            }
        }
        i = i + 1;
    }
    t = t * 10 + 4;
    return t;
}

// run: test_loopif_while_chain(0) == 2334
// run: test_loopif_while_chain(1) == 1234
// run: test_loopif_while_chain(2) == 1124
// run: test_loopif_while_chain(3) == 1114
