// test run

// ============================================================================
// Control-flow torture: if/else inside while loop
// 3-iteration while loop; then-arm while i < p, else-arm after.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_loopif_while_ifelse(int p) {
    int t = 0;
    int i = 0;
    while (i < 3) {
        if (i < p) {
            t = t * 10 + 1;
        } else {
            t = t * 10 + 2;
        }
        i = i + 1;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_loopif_while_ifelse(0) == 2223
// run: test_loopif_while_ifelse(1) == 1223
// run: test_loopif_while_ifelse(2) == 1123
// run: test_loopif_while_ifelse(3) == 1113
