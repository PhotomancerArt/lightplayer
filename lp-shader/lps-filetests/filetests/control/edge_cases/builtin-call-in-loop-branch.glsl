// test run

// ============================================================================
// Regression: builtin call in a branch that falls through (2026-07-27).
//
// Bisected from a live shader-agent session: `abs()` on a float inside the
// if-branch of a for loop, where the same branch also stores into an array,
// failed to compile on the WASM backend with
// "values remaining on stack at end of block" (Chrome words it "expected 0
// elements on the stack for fallthru, found 2").
//
// Root cause was `lpvm_wasm::emit::q32::emit_q32_fabs` pushing `src` ahead of
// its own condition, leaking one operand per `abs`. It went unnoticed because
// WASM validation goes polymorphic after `return`: the leak is invisible in a
// straight-line function and only trips validation when the enclosing block
// falls through, as it does here.
//
// The systematic version of this shape, over every builtin, is
// `control/torture/intrin_*.glsl`.
// ============================================================================

float test_abs_in_loop_branch(int count) {
    float bpos[8];
    for (int i = 0; i < 8; i++) {
        if (i < count) {
            float m = mod(float(i), 2.0);
            float tri = abs(m - 1.0);
            bpos[i] = 1.0 + 2.0 * tri;
        } else {
            bpos[i] = 0.0;
        }
    }
    return bpos[0] + bpos[1] + bpos[2];
}

// run: test_abs_in_loop_branch(3) ~= 7.0
// run: test_abs_in_loop_branch(0) ~= 0.0

// The original report noted the failure persisted even when the result was
// multiplied by zero, i.e. the call is what matters, not its value.
float test_abs_result_discarded(int count) {
    float bpos[8];
    for (int i = 0; i < 8; i++) {
        bpos[i] = 0.0;
        if (i < count) {
            float m = mod(float(i), 2.0);
            float tri = abs(m - 1.0);
            bpos[i] = 1.0 + 0.0 * tri;
        }
    }
    return bpos[0] + bpos[1] + bpos[2];
}

// run: test_abs_result_discarded(3) ~= 3.0
// run: test_abs_result_discarded(1) ~= 1.0
