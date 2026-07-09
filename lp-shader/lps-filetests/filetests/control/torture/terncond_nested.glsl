// test run

// ============================================================================
// Control-flow torture: nested ternaries as branch conditions
// A ternary whose condition is itself built from a ternary, and a
// ternary with boolean arms used directly as an if condition.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_terncond_nested(int a, int b, int c) {
    int t = 0;
    if (((a > 0 ? b : c) > 0 ? 1 : 0) > 0) {
        t = t * 10 + 1;
    } else {
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_terncond_nested(0, 0, 0) == 23
// run: test_terncond_nested(0, 0, 1) == 13
// run: test_terncond_nested(0, 1, 0) == 23
// run: test_terncond_nested(0, 1, 1) == 13
// run: test_terncond_nested(1, 0, 0) == 23
// run: test_terncond_nested(1, 0, 1) == 23
// run: test_terncond_nested(1, 1, 0) == 13
// run: test_terncond_nested(1, 1, 1) == 13

int test_terncond_bool_arms(int a, int b, int c) {
    int t = 0;
    if ((a > 0 ? b > 0 : c > 0)) {
        t = t * 10 + 1;
    } else {
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_terncond_bool_arms(0, 0, 0) == 23
// run: test_terncond_bool_arms(0, 0, 1) == 13
// run: test_terncond_bool_arms(0, 1, 0) == 23
// run: test_terncond_bool_arms(0, 1, 1) == 13
// run: test_terncond_bool_arms(1, 0, 0) == 23
// run: test_terncond_bool_arms(1, 0, 1) == 23
// run: test_terncond_bool_arms(1, 1, 0) == 13
// run: test_terncond_bool_arms(1, 1, 1) == 13
