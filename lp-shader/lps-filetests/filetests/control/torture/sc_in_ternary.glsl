// test run

// ============================================================================
// Control-flow torture: short-circuit operators as ternary conditions
// (chk && chk) ? 7 : 8 and the || variant.
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

int test_sc_in_ternary(int a, int b) {
    g_trace = 0;
    int r = (chk(1, a) && chk(2, b) ? 7 : 8);
    return g_trace * 10 + r;
}

// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_ternary(0, 0) == 18
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_ternary(0, 1) == 18
// run: test_sc_in_ternary(1, 0) == 128
// run: test_sc_in_ternary(1, 1) == 127

int test_sc_in_ternary_or(int a, int b) {
    g_trace = 0;
    int r = (chk(1, a) || chk(2, b) ? 7 : 8);
    return g_trace * 10 + r;
}

// run: test_sc_in_ternary_or(0, 0) == 128
// run: test_sc_in_ternary_or(0, 1) == 127
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_ternary_or(1, 0) == 17
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_in_ternary_or(1, 1) == 17
