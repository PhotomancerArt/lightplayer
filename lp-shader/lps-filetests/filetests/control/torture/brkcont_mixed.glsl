// test run

// ============================================================================
// Control-flow torture: break and continue split across nested loop levels
// continue in the outer loop with break in the inner loop, and the
// reverse; each must bind to its own loop.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_cont_outer_brk_inner(int p, int q) {
    int t = 0;
    for (int i = 0; i < 2; i++) {
        if (i == p) {
            continue;
        }
        t = t * 10 + 1;
        int j = 0;
        while (j < 2) {
            if (j == q) {
                break;
            }
            t = t * 10 + 2;
            j = j + 1;
        }
        t = t * 10 + 3;
    }
    t = t * 10 + 4;
    return t;
}

// run: test_cont_outer_brk_inner(0, 0) == 134
// run: test_cont_outer_brk_inner(0, 1) == 1234
// run: test_cont_outer_brk_inner(0, 2) == 12234
// run: test_cont_outer_brk_inner(1, 0) == 134
// run: test_cont_outer_brk_inner(1, 1) == 1234
// run: test_cont_outer_brk_inner(1, 2) == 12234
// run: test_cont_outer_brk_inner(2, 0) == 13134
// run: test_cont_outer_brk_inner(2, 1) == 1231234
// run: test_cont_outer_brk_inner(2, 2) == 122312234

int test_brk_outer_cont_inner(int p, int q) {
    int t = 0;
    int i = 0;
    while (i < 3) {
        i = i + 1;
        if (i == p) {
            break;
        }
        t = t * 10 + 1;
        for (int j = 0; j < 2; j++) {
            if (j == q) {
                continue;
            }
            t = t * 10 + 2;
        }
    }
    t = t * 10 + 3;
    return t;
}

// run: test_brk_outer_cont_inner(1, 0) == 3
// run: test_brk_outer_cont_inner(1, 1) == 3
// run: test_brk_outer_cont_inner(1, 2) == 3
// run: test_brk_outer_cont_inner(2, 0) == 123
// run: test_brk_outer_cont_inner(2, 1) == 123
// run: test_brk_outer_cont_inner(2, 2) == 1223
// run: test_brk_outer_cont_inner(4, 0) == 1212123
// run: test_brk_outer_cont_inner(4, 1) == 1212123
// run: test_brk_outer_cont_inner(4, 2) == 1221221223
