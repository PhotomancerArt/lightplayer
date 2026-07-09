// test run

// ============================================================================
// Control-flow torture: else-if chain nested inside an else-if chain arm
// Outer 3-way chain on p; arm 0 holds another 3-way chain on q.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ifnest_chain_d3(int p, int q) {
    int t = 0;
    if (p == 0) {
        if (q == 0) {
            t = t * 10 + 1;
        } else {
            if (q == 1) {
                t = t * 10 + 2;
            } else {
                t = t * 10 + 3;
            }
        }
    } else {
        if (p == 1) {
            t = t * 10 + 4;
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

// run: test_ifnest_chain_d3(0, 0) == 17
// run: test_ifnest_chain_d3(0, 1) == 27
// run: test_ifnest_chain_d3(0, 2) == 37
// run: test_ifnest_chain_d3(1, 0) == 47
// run: test_ifnest_chain_d3(1, 1) == 47
// run: test_ifnest_chain_d3(1, 2) == 47
// run: test_ifnest_chain_d3(2, 0) == 67
// run: test_ifnest_chain_d3(2, 1) == 57
// run: test_ifnest_chain_d3(2, 2) == 57
