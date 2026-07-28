// test run

// ============================================================================
// Control-flow torture: break/continue at three nesting depths
// for/for/for: innermost break, middle continue, outer break;
// while/for/do-while: innermost continue then break. Each must
// bind to its own loop.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_brknest_triple_for(int p, int q, int r) {
    int t = 0;
    for (int i = 0; i < 2; i++) {
        for (int j = 0; j < 2; j++) {
            if (j == q) {
                continue;
            }
            for (int k = 0; k < 2; k++) {
                if (k == p) {
                    break;
                }
                t = t * 10 + 1;
            }
        }
        if (i == r) {
            break;
        }
    }
    t = t * 10 + 2;
    return t;
}

// run: test_brknest_triple_for(0, 0, 0) == 2
// run: test_brknest_triple_for(0, 0, 2) == 2
// run: test_brknest_triple_for(0, 1, 0) == 2
// run: test_brknest_triple_for(0, 1, 2) == 2
// run: test_brknest_triple_for(0, 2, 0) == 2
// run: test_brknest_triple_for(0, 2, 2) == 2
// run: test_brknest_triple_for(1, 0, 0) == 12
// run: test_brknest_triple_for(1, 0, 2) == 112
// run: test_brknest_triple_for(1, 1, 0) == 12
// run: test_brknest_triple_for(1, 1, 2) == 112
// run: test_brknest_triple_for(1, 2, 0) == 112
// run: test_brknest_triple_for(1, 2, 2) == 11112
// run: test_brknest_triple_for(2, 0, 0) == 112
// run: test_brknest_triple_for(2, 0, 2) == 11112
// run: test_brknest_triple_for(2, 1, 0) == 112
// run: test_brknest_triple_for(2, 1, 2) == 11112
// run: test_brknest_triple_for(2, 2, 0) == 11112
// run: test_brknest_triple_for(2, 2, 2) == 111111112

int test_brknest_triple_mixed(int p, int q) {
    int t = 0;
    int i = 0;
    while (i < 2) {
        i = i + 1;
        for (int j = 0; j < 2; j++) {
            int k = 0;
            do {
                k = k + 1;
                if (k == p) {
                    continue;
                }
                t = t * 10 + 1;
                if (k == q) {
                    break;
                }
            } while (k < 2);
        }
    }
    t = t * 10 + 2;
    return t;
}

// run: test_brknest_triple_mixed(1, 1) == 11112
// run: test_brknest_triple_mixed(1, 2) == 11112
// run: test_brknest_triple_mixed(1, 3) == 11112
// run: test_brknest_triple_mixed(2, 1) == 11112
// run: test_brknest_triple_mixed(2, 2) == 11112
// run: test_brknest_triple_mixed(2, 3) == 11112
// run: test_brknest_triple_mixed(3, 1) == 11112
// run: test_brknest_triple_mixed(3, 2) == 111111112
// run: test_brknest_triple_mixed(3, 3) == 111111112
