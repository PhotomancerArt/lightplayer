// test run

// ============================================================================
// Mix: mix(bvec2, bvec2, bvec2) -> bvec2 (component-wise selection)
// ============================================================================

bvec2 test_bvec2_mix_all_false_selector() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    bvec2 selector = bvec2(false, false);
    // Function mix() returns bvec2 (component-wise selection)
    // For each component: if selector is false, take from first arg; if true, take from second arg
    return mix(a, b, selector);
}

// @unimplemented(jit.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec2_mix_all_false_selector() == bvec2(true, false)

bvec2 test_bvec2_mix_all_true_selector() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    bvec2 selector = bvec2(true, true);
    return mix(a, b, selector);
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec2_mix_all_true_selector() == bvec2(false, true)

bvec2 test_bvec2_mix_mixed_selector() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    bvec2 selector = bvec2(false, true);
    return mix(a, b, selector);
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec2_mix_mixed_selector() == bvec2(true, true)

bvec2 test_bvec2_mix_other_mixed_selector() {
    bvec2 a = bvec2(false, true);
    bvec2 b = bvec2(true, false);
    bvec2 selector = bvec2(true, false);
    return mix(a, b, selector);
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec2_mix_other_mixed_selector() == bvec2(true, true)

bvec2 test_bvec2_mix_same_vectors() {
    bvec2 a = bvec2(true, true);
    bvec2 selector = bvec2(false, true);
    return mix(a, a, selector);
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec2_mix_same_vectors() == bvec2(true, true)

bvec2 test_bvec2_mix_in_expression() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    bvec2 selector = bvec2(true, false);
    bvec2 result = mix(a, b, selector);
    return not(result);
    // mix((true,false), (false,true), (true,false)) = (false, false)
    // not((false, false)) = (true, true)
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec2_mix_in_expression() == bvec2(true, true)
