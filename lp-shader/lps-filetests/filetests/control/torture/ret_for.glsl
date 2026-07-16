// test run

// ============================================================================
// Control-flow torture: early return out of a for loop
// Return from the then arm mid-iteration and from an else arm;
// the post-loop site must not run on returning paths.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

int test_ret_for(int p) {
    int t = 0;
    for (int i = 0; i < 3; i++) {
        t = t * 10 + 1;
        if (i == p) {
            return t;
        }
        t = t * 10 + 2;
    }
    t = t * 10 + 3;
    return t;
}

// run: test_ret_for(0) == 1
// @unsupported(wgpu.f32)
// run: test_ret_for(1) == 121
// wgpu.f32: f32 GPU result diverges (undefined/edge-domain semantics)
// @unsupported(wgpu.f32)
// run: test_ret_for(2) == 12121
// run: test_ret_for(3) == 1212123

int test_ret_for_else(int p) {
    int t = 0;
    for (int i = 0; i < 3; i++) {
        if (i != p) {
            t = t * 10 + 1;
        } else {
            return t;
        }
    }
    t = t * 10 + 2;
    return t;
}

// run: test_ret_for_else(0) == 0
// run: test_ret_for_else(1) == 1
// run: test_ret_for_else(2) == 11
// run: test_ret_for_else(3) == 1112
