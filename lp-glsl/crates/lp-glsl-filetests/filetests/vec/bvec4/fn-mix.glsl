// test run
// target riscv32.q32

// ============================================================================
// Mix: mix(bvec4, bvec4, bvec4) -> bvec4 (component-wise selection)
// ============================================================================

bvec4 test_bvec4_mix_all_false_selector() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    bvec4 selector = bvec4(false, false, false, false);
    // Function mix() returns bvec4 (component-wise selection)
    // For each component: if selector is false, take from first arg; if true, take from second arg
    return mix(a, b, selector);
}

// run: test_bvec4_mix_all_false_selector() == bvec4(true, false, true, false)

bvec4 test_bvec4_mix_all_true_selector() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    bvec4 selector = bvec4(true, true, true, true);
    return mix(a, b, selector);
}

// run: test_bvec4_mix_all_true_selector() == bvec4(false, true, false, true)

bvec4 test_bvec4_mix_mixed_selector() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    bvec4 selector = bvec4(false, true, false, true);
    return mix(a, b, selector);
}

// run: test_bvec4_mix_mixed_selector() == bvec4(true, true, true, true)

bvec4 test_bvec4_mix_other_mixed_selector() {
    bvec4 a = bvec4(false, true, false, true);
    bvec4 b = bvec4(true, false, true, false);
    bvec4 selector = bvec4(true, false, true, false);
    return mix(a, b, selector);
}

// run: test_bvec4_mix_other_mixed_selector() == bvec4(true, true, true, true)

bvec4 test_bvec4_mix_same_vectors() {
    bvec4 a = bvec4(true, true, true, true);
    bvec4 selector = bvec4(false, true, false, true);
    return mix(a, a, selector);
}

// run: test_bvec4_mix_same_vectors() == bvec4(true, true, true, true)

bvec4 test_bvec4_mix_in_expression() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 b = bvec4(false, true, false, true);
    bvec4 selector = bvec4(true, false, true, false);
    bvec4 result = mix(a, b, selector);
    return not(result);
    // mix((true,false,true,false), (false,true,false,true), (true,false,true,false)) = (false, false, false, false)
    // not((false, false, false, false)) = (true, true, true, true)
}

// run: test_bvec4_mix_in_expression() == bvec4(true, true, true, true)
