// test run

// ============================================================================
// Lifecycle: Multiple globals of different types reset correctly
// ============================================================================

float gf = 1.0;
int gi = 10;
vec2 gv = vec2(2.0, 3.0);

float test_multi_type_mutate_and_read() {
    float result = gf + float(gi) + gv.x + gv.y;
    gf = 99.0;
    gi = 99;
    gv = vec2(99.0, 99.0);
    return result;
}

// Both calls should return 1.0 + 10.0 + 2.0 + 3.0 = 16.0
// @unimplemented(wasm.q32)
// run: test_multi_type_mutate_and_read() ~= 16.0
// @unimplemented(wasm.q32)
// run: test_multi_type_mutate_and_read() ~= 16.0
