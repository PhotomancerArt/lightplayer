// test run

// ============================================================================
// Control-flow torture: ternary inside loop conditions
// The loop bound itself is a ternary; it is re-evaluated on every
// iteration test.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_terncond_for(int p) {
    int t = 0;
    for (int i = 0; i < (p > 0 ? 2 : 3); i++) {
        t = t * 10 + 1;
    }
    t = t * 10 + 2;
    return t;
}

// run: test_terncond_for(0) == 1112
// run: test_terncond_for(1) == 112

int test_terncond_while(int p, int q) {
    int t = 0;
    int i = 0;
    while (i < (p > 0 ? q : 2)) {
        t = t * 10 + 1;
        i = i + 1;
    }
    t = t * 10 + 2;
    return t;
}

// run: test_terncond_while(0, 0) == 112
// run: test_terncond_while(0, 1) == 112
// run: test_terncond_while(0, 3) == 112
// run: test_terncond_while(1, 0) == 2
// run: test_terncond_while(1, 1) == 12
// run: test_terncond_while(1, 3) == 1112

int test_terncond_dowhile(int p) {
    int t = 0;
    int i = 0;
    do {
        t = t * 10 + 1;
        i = i + 1;
    } while (i < (p > 0 ? 3 : 1));
    t = t * 10 + 2;
    return t;
}

// run: test_terncond_dowhile(0) == 12
// run: test_terncond_dowhile(1) == 1112
