// test run

// ============================================================================
// Control-flow torture: if/else inside for loop
// 3-iteration for loop; then-arm while i < p, else-arm after.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_loopif_for_ifelse(int p) {
    int t = 0;
    for (int i = 0; i < 3; i++) {
        if (i < p) {
            t = t * 10 + 1;
        } else {
            t = t * 10 + 2;
        }
    }
    t = t * 10 + 3;
    return t;
}

// run: test_loopif_for_ifelse(0) == 2223
// run: test_loopif_for_ifelse(1) == 1223
// run: test_loopif_for_ifelse(2) == 1123
// run: test_loopif_for_ifelse(3) == 1113
