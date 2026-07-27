// test run

// ============================================================================
// Control-flow torture: break/continue across a for loop nesting a dowhile loop
// inner body carries both continue and break (must bind inner);
// outer break and outer continue placed after the inner loop (must
// bind outer). while/do-while induction increments precede the body
// wherever that body can continue.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_brknest_for_dowhile_inner_both(int p, int q) {
    int t = 0;
    for (int i = 0; i < 2; i++) {
        int j = 0;
        do {
            j = j + 1;
            if (j == p) {
                continue;
            }
            t = t * 10 + 1;
            if (j == q) {
                break;
            }
            t = t * 10 + 2;
        } while (j < 2);
    }
    t = t * 10 + 3;
    return t;
}

// run: test_brknest_for_dowhile_inner_both(0, 0) == 121212123
// run: test_brknest_for_dowhile_inner_both(0, 2) == 1211213
// run: test_brknest_for_dowhile_inner_both(0, 3) == 121212123
// run: test_brknest_for_dowhile_inner_both(1, 0) == 12123
// run: test_brknest_for_dowhile_inner_both(1, 2) == 113
// run: test_brknest_for_dowhile_inner_both(1, 3) == 12123
// run: test_brknest_for_dowhile_inner_both(3, 0) == 121212123
// run: test_brknest_for_dowhile_inner_both(3, 2) == 1211213
// run: test_brknest_for_dowhile_inner_both(3, 3) == 121212123

int test_brknest_for_dowhile_outer_brk(int p, int q) {
    int t = 0;
    for (int i = 0; i < 2; i++) {
        t = t * 10 + 1;
        int j = 0;
        do {
            j = j + 1;
            if (j == q) {
                continue;
            }
            t = t * 10 + 2;
        } while (j < 2);
        if (i == p) {
            break;
        }
        t = t * 10 + 3;
    }
    t = t * 10 + 4;
    return t;
}

// run: test_brknest_for_dowhile_outer_brk(0, 0) == 1224
// run: test_brknest_for_dowhile_outer_brk(0, 1) == 124
// run: test_brknest_for_dowhile_outer_brk(0, 2) == 124
// run: test_brknest_for_dowhile_outer_brk(1, 0) == 12231224
// run: test_brknest_for_dowhile_outer_brk(1, 1) == 123124
// run: test_brknest_for_dowhile_outer_brk(1, 2) == 123124
// run: test_brknest_for_dowhile_outer_brk(3, 0) == 122312234
// run: test_brknest_for_dowhile_outer_brk(3, 1) == 1231234
// run: test_brknest_for_dowhile_outer_brk(3, 2) == 1231234

int test_brknest_for_dowhile_outer_cont(int p, int q) {
    int t = 0;
    for (int i = 0; i < 2; i++) {
        t = t * 10 + 1;
        int j = 0;
        do {
            if (j == q) {
                break;
            }
            t = t * 10 + 2;
            j = j + 1;
        } while (j < 2);
        if (i == p) {
            continue;
        }
        t = t * 10 + 3;
    }
    t = t * 10 + 4;
    return t;
}

// run: test_brknest_for_dowhile_outer_cont(0, 0) == 1134
// run: test_brknest_for_dowhile_outer_cont(0, 1) == 121234
// run: test_brknest_for_dowhile_outer_cont(0, 2) == 12212234
// run: test_brknest_for_dowhile_outer_cont(1, 0) == 1314
// run: test_brknest_for_dowhile_outer_cont(1, 1) == 123124
// run: test_brknest_for_dowhile_outer_cont(1, 2) == 12231224
// run: test_brknest_for_dowhile_outer_cont(3, 0) == 13134
// run: test_brknest_for_dowhile_outer_cont(3, 1) == 1231234
// run: test_brknest_for_dowhile_outer_cont(3, 2) == 122312234
