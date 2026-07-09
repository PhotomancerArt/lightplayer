// test run

// ============================================================================
// Control-flow torture: mixed && / || chains with side effects
// a && b || c, a || b && c, and a && (b || c): the skip set depends
// on precedence and grouping.
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

int test_sc_and_or(int a, int b, int c) {
    g_trace = 0;
    int r = (chk(1, a) && chk(2, b) || chk(3, c) ? 1 : 2);
    return g_trace * 10 + r;
}

// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_or(0, 0, 0) == 132
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_or(0, 0, 1) == 131
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_or(0, 1, 0) == 132
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_or(0, 1, 1) == 131
// run: test_sc_and_or(1, 0, 0) == 1232
// run: test_sc_and_or(1, 0, 1) == 1231
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_or(1, 1, 0) == 121
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_or(1, 1, 1) == 121

int test_sc_or_and(int a, int b, int c) {
    g_trace = 0;
    int r = (chk(1, a) || chk(2, b) && chk(3, c) ? 1 : 2);
    return g_trace * 10 + r;
}

// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_or_and(0, 0, 0) == 122
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_or_and(0, 0, 1) == 122
// run: test_sc_or_and(0, 1, 0) == 1232
// run: test_sc_or_and(0, 1, 1) == 1231
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_or_and(1, 0, 0) == 11
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_or_and(1, 0, 1) == 11
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_or_and(1, 1, 0) == 11
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_or_and(1, 1, 1) == 11

int test_sc_and_grouped_or(int a, int b, int c) {
    g_trace = 0;
    int r = (chk(1, a) && (chk(2, b) || chk(3, c)) ? 1 : 2);
    return g_trace * 10 + r;
}

// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_grouped_or(0, 0, 0) == 12
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_grouped_or(0, 0, 1) == 12
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_grouped_or(0, 1, 0) == 12
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_grouped_or(0, 1, 1) == 12
// run: test_sc_and_grouped_or(1, 0, 0) == 1232
// run: test_sc_and_grouped_or(1, 0, 1) == 1231
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_grouped_or(1, 1, 0) == 121
// @broken(rv32n.q32)
// @broken(rv32c.q32)
// @broken(wasm.q32)
// run: test_sc_and_grouped_or(1, 1, 1) == 121
