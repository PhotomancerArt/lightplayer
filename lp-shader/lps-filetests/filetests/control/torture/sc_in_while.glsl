// test run

// ============================================================================
// Control-flow torture: side-effecting while conditions
// The condition call must run exactly once per test, including the
// final failing test; the RHS of && must be skipped once i reaches 3.
//
// KNOWN BUG: GLSL requires && / || to short-circuit, but the current
// frontend lowering evaluates both operands (docs/design/lpir/02-core-ops.md
// documents the gap; docs/design/lpir/08-glsl-mapping.md says side-effecting
// cases must lower to control flow). Expected values below are the
// GLSL-correct short-circuit results; directives whose value would differ
// under eager evaluation are marked @broken until the lowering is fixed.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int g_trace = 0;

bool chk(int k, int v) {
    g_trace = g_trace * 10 + k;
    return v > 0;
}

int test_sc_in_while(int p) {
    g_trace = 0;
    int i = 0;
    while (chk(1, p - i)) {
        i = i + 1;
    }
    return g_trace * 10 + i;
}

// run: test_sc_in_while(0) == 10
// run: test_sc_in_while(1) == 111
// run: test_sc_in_while(2) == 1112
// run: test_sc_in_while(3) == 11113

int test_sc_in_while_and(int p) {
    g_trace = 0;
    int i = 0;
    while (i < 3 && chk(2, p - i)) {
        i = i + 1;
    }
    return g_trace * 10 + i;
}

// run: test_sc_in_while_and(0) == 20
// run: test_sc_in_while_and(1) == 221
// run: test_sc_in_while_and(2) == 2222
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_while_and(3) == 2223
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_while_and(4) == 2223
