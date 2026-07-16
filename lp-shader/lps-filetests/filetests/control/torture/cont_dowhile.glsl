// test run

// ============================================================================
// Control-flow torture: continue in dowhile loops
// continue guarded in then arm / else arm at depth 1; continue in the
// inner loop of a nested pair (must re-test the inner condition only).
// For while/do-while the induction increment precedes the continue,
// exercising the continue-to-condition edge.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_cont_dowhile_d1(int p) {
    int t = 0;
    int i = 0;
    do {
        t = t * 10 + 1;
        i = i + 1;
        if (i == p) {
            continue;
        }
        t = t * 10 + 2;
    } while (i < 3);
    t = t * 10 + 3;
    return t;
}

// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d1(0) == 1212123
// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d1(1) == 112123
// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d1(2) == 121123
// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d1(3) == 121213

int test_cont_dowhile_d1_else(int p) {
    int t = 0;
    int i = 0;
    do {
        i = i + 1;
        if (i == p) {
            t = t * 10 + 1;
        } else {
            continue;
        }
        t = t * 10 + 2;
    } while (i < 3);
    t = t * 10 + 3;
    return t;
}

// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d1_else(0) == 3
// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d1_else(1) == 123
// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d1_else(2) == 123
// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d1_else(3) == 123

int test_cont_dowhile_d2_inner(int p) {
    int t = 0;
    for (int i = 0; i < 2; i++) {
        t = t * 10 + 1;
        int j = 0;
        do {
            j = j + 1;
            if (j == p) {
                continue;
            }
            t = t * 10 + 2;
        } while (j < 2);
        t = t * 10 + 3;
    }
    t = t * 10 + 4;
    return t;
}

// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d2_inner(0) == 122312234
// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d2_inner(1) == 1231234
// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d2_inner(2) == 1231234
// wgpu.f32: shader does not terminate on the GPU tier (no fuel; CPU targets rely on fuel-exhaustion traps)
// @unsupported(wgpu.f32)
// run: test_cont_dowhile_d2_inner(3) == 122312234
