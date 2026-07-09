// test run

// ============================================================================
// Control-flow torture: else-if chain with nested if/else in every arm
// 3-way else-if chain on p; each arm holds an if/else on q.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifnest_chain_d2(int p, int q) {
    int t = 0;
    if (p == 0) {
        if (q > 0) {
            t = t * 10 + 1;
        } else {
            t = t * 10 + 2;
        }
    } else {
        if (p == 1) {
            if (q > 0) {
                t = t * 10 + 3;
            } else {
                t = t * 10 + 4;
            }
        } else {
            if (q > 0) {
                t = t * 10 + 5;
            } else {
                t = t * 10 + 6;
            }
        }
    }
    t = t * 10 + 7;
    return t;
}

// run: test_ifnest_chain_d2(0, 0) == 27
// run: test_ifnest_chain_d2(0, 1) == 17
// run: test_ifnest_chain_d2(1, 0) == 47
// run: test_ifnest_chain_d2(1, 1) == 37
// run: test_ifnest_chain_d2(2, 0) == 67
// run: test_ifnest_chain_d2(2, 1) == 57
