// test run

// ============================================================================
// Mix: mix(bvec3, bvec3, bvec3) -> bvec3 (component-wise selection)
// ============================================================================

bvec3 test_bvec3_mix_all_false_selector() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, false);
    bvec3 selector = bvec3(false, false, false);
    // Function mix() returns bvec3 (component-wise selection)
    // For each component: if selector is false, take from first arg; if true, take from second arg
    return mix(a, b, selector);
}

// @unimplemented(jit.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec3_mix_all_false_selector() == bvec3(true, false, true)

bvec3 test_bvec3_mix_all_true_selector() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, false);
    bvec3 selector = bvec3(true, true, true);
    return mix(a, b, selector);
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec3_mix_all_true_selector() == bvec3(false, true, false)

bvec3 test_bvec3_mix_mixed_selector() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, false);
    bvec3 selector = bvec3(false, true, false);
    return mix(a, b, selector);
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec3_mix_mixed_selector() == bvec3(true, true, true)

bvec3 test_bvec3_mix_other_mixed_selector() {
    bvec3 a = bvec3(false, true, false);
    bvec3 b = bvec3(true, false, true);
    bvec3 selector = bvec3(true, false, true);
    return mix(a, b, selector);
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec3_mix_other_mixed_selector() == bvec3(true, true, true)

bvec3 test_bvec3_mix_same_vectors() {
    bvec3 a = bvec3(true, true, true);
    bvec3 selector = bvec3(false, true, false);
    return mix(a, a, selector);
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec3_mix_same_vectors() == bvec3(true, true, true)

bvec3 test_bvec3_mix_in_expression() {
    bvec3 a = bvec3(true, false, true);
    bvec3 b = bvec3(false, true, false);
    bvec3 selector = bvec3(true, false, true);
    bvec3 result = mix(a, b, selector);
    return not(result);
    // mix((true,false,true), (false,true,false), (true,false,true)) = (false, false, false)
    // not((false, false, false)) = (true, true, true)
}

// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// run: test_bvec3_mix_in_expression() == bvec3(true, true, true)
