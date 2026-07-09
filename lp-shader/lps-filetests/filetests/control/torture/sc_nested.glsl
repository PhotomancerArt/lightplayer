// test run

// ============================================================================
// Control-flow torture: (a && b) || (c && d) with side effects
// All 16 input combinations; each has a distinct evaluation trace.
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

int test_sc_nested_groups(int a, int b, int c, int d) {
    g_trace = 0;
    int r = (chk(1, a) && chk(2, b) || chk(3, c) && chk(4, d) ? 1 : 2);
    return g_trace * 10 + r;
}

// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(0, 0, 0, 0) == 132
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(0, 0, 0, 1) == 132
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(0, 0, 1, 0) == 1342
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(0, 0, 1, 1) == 1341
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(0, 1, 0, 0) == 132
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(0, 1, 0, 1) == 132
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(0, 1, 1, 0) == 1342
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(0, 1, 1, 1) == 1341
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(1, 0, 0, 0) == 1232
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(1, 0, 0, 1) == 1232
// run: test_sc_nested_groups(1, 0, 1, 0) == 12342
// run: test_sc_nested_groups(1, 0, 1, 1) == 12341
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(1, 1, 0, 0) == 121
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(1, 1, 0, 1) == 121
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(1, 1, 1, 0) == 121
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_nested_groups(1, 1, 1, 1) == 121
