// test run

// ============================================================================
// Control-flow torture: short-circuit operators as if conditions
// The branch taken and the evaluation trace must both be right.
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

int test_sc_in_if_and(int a, int b) {
    int t = 0;
    g_trace = 0;
    if (chk(1, a) && chk(2, b)) {
        t = t * 10 + 1;
    } else {
        t = t * 10 + 2;
    }
    return g_trace * 10 + t;
}

// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_if_and(0, 0) == 12
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_if_and(0, 1) == 12
// run: test_sc_in_if_and(1, 0) == 122
// run: test_sc_in_if_and(1, 1) == 121

int test_sc_in_if_or(int a, int b) {
    int t = 0;
    g_trace = 0;
    if (chk(1, a) || chk(2, b)) {
        t = t * 10 + 1;
    } else {
        t = t * 10 + 2;
    }
    return g_trace * 10 + t;
}

// run: test_sc_in_if_or(0, 0) == 122
// run: test_sc_in_if_or(0, 1) == 121
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_if_or(1, 0) == 11
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_if_or(1, 1) == 11
