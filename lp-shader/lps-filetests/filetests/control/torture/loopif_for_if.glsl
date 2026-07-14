// test run

// ============================================================================
// Control-flow torture: if inside for loop
// 3-iteration for loop; if taken while i < p.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_loopif_for_if(int p) {
    int t = 0;
    for (int i = 0; i < 3; i++) {
        if (i < p) {
            t = t * 10 + 1;
        }
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_loopif_for_if(0) == 2223
// run: test_loopif_for_if(1) == 12223
// run: test_loopif_for_if(2) == 121223
// run: test_loopif_for_if(3) == 1212123
