// test run

// ============================================================================
// Control-flow torture: break in while loops
// break guarded in then arm / else arm at depth 1; break in the inner
// loop of a nested pair (must exit inner only); break behind a
// depth-2 if guard.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_brk_while_d1_then(int p) {
    int t = 0;
    int i = 0;
    while (i < 4) {
        t = t * 10 + 1;
        if (i == p) {
            break;
        }
        t = t * 10 + 2;
        i = i + 1;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_brk_while_d1_then(0) == 13
// run: test_brk_while_d1_then(1) == 1213
// run: test_brk_while_d1_then(2) == 121213
// run: test_brk_while_d1_then(3) == 12121213
// run: test_brk_while_d1_then(4) == 121212123

int test_brk_while_d1_else(int p) {
    int t = 0;
    int i = 0;
    while (i < 4) {
        if (i != p) {
            t = t * 10 + 1;
        } else {
            break;
        }
        i = i + 1;
    }
    t = t * 10 + 2;
    return t;
}

// run: test_brk_while_d1_else(0) == 2
// run: test_brk_while_d1_else(1) == 12
// run: test_brk_while_d1_else(2) == 112
// run: test_brk_while_d1_else(3) == 1112
// run: test_brk_while_d1_else(4) == 11112

int test_brk_while_d2_inner(int p) {
    int t = 0;
    int i = 0;
    while (i < 2) {
        t = t * 10 + 1;
        int j = 0;
        while (j < 2) {
            if (j == p) {
                break;
            }
            t = t * 10 + 2;
            j = j + 1;
        }
        t = t * 10 + 3;
        i = i + 1;
    }
    t = t * 10 + 4;
    return t;
}

// run: test_brk_while_d2_inner(0) == 13134
// run: test_brk_while_d2_inner(1) == 1231234
// run: test_brk_while_d2_inner(2) == 122312234

int test_brk_while_d2_guard(int p, int q) {
    int t = 0;
    int i = 0;
    while (i < 4) {
        if (i >= p) {
            if (q > 0) {
                break;
            }
        }
        t = t * 10 + 1;
        i = i + 1;
    }
    t = t * 10 + 2;
    return t;
}

// run: test_brk_while_d2_guard(0, 0) == 11112
// run: test_brk_while_d2_guard(0, 1) == 2
// run: test_brk_while_d2_guard(2, 0) == 11112
// run: test_brk_while_d2_guard(2, 1) == 112
// run: test_brk_while_d2_guard(4, 0) == 11112
// run: test_brk_while_d2_guard(4, 1) == 11112
