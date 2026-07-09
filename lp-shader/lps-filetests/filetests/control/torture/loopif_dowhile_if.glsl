// test run

// ============================================================================
// Control-flow torture: if inside dowhile loop
// 3-iteration dowhile loop; if taken while i < p.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_loopif_dowhile_if(int p) {
    int t = 0;
    int i = 0;
    do {
        if (i < p) {
            t = t * 10 + 1;
        }
        t = t * 10 + 2;
        i = i + 1;
    } while (i < 3);
    t = t * 10 + 3;
    return t;
}

// run: test_loopif_dowhile_if(0) == 2223
// run: test_loopif_dowhile_if(1) == 12223
// run: test_loopif_dowhile_if(2) == 121223
// run: test_loopif_dowhile_if(3) == 1212123
