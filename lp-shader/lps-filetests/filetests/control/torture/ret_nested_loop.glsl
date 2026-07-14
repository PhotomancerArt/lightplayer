// test run

// ============================================================================
// Control-flow torture: early return from the inner loop of a nested pair
// Return fires when (i, j) == (p, q); both loops unwind at once.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ret_nested_loop(int p, int q) {
    int t = 0;
    for (int i = 0; i < 3; i++) {
        t = t * 10 + 1;
        int j = 0;
        while (j < 2) {
            if (i == p && j == q) {
                return t;
            }
            t = t * 10 + 2;
            j = j + 1;
        }
    }
    t = t * 10 + 3;
    return t;
}

// run: test_ret_nested_loop(0, 0) == 1
// run: test_ret_nested_loop(0, 1) == 12
// run: test_ret_nested_loop(1, 0) == 1221
// run: test_ret_nested_loop(1, 1) == 12212
// run: test_ret_nested_loop(2, 0) == 1221221
// run: test_ret_nested_loop(2, 1) == 12212212
// run: test_ret_nested_loop(3, 0) == 1221221223
// run: test_ret_nested_loop(3, 1) == 1221221223
