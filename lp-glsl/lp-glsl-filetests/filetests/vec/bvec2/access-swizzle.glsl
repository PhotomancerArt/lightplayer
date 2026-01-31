// test run
// target riscv32.q32

// ============================================================================
// Access Swizzle: bvec2.xy, bvec2.yx, bvec2.xx, etc. - component swizzling
// ============================================================================

bvec2 test_bvec2_access_swizzle_xy() {
    // Identity swizzle
    bvec2 a = bvec2(true, false);
    return a.xy;
}

// run: test_bvec2_access_swizzle_xy() == bvec2(true, false)

bvec2 test_bvec2_access_swizzle_yx() {
    // Reverse swizzle
    bvec2 a = bvec2(true, false);
    return a.yx;
}

// run: test_bvec2_access_swizzle_yx() == bvec2(false, true)

bvec2 test_bvec2_access_swizzle_xx() {
    // Duplicate swizzle
    bvec2 a = bvec2(true, false);
    return a.xx;
}

// run: test_bvec2_access_swizzle_xx() == bvec2(true, true)

bvec2 test_bvec2_access_swizzle_yy() {
    bvec2 a = bvec2(true, false);
    return a.yy;
}

// run: test_bvec2_access_swizzle_yy() == bvec2(false, false)

bvec2 test_bvec2_access_swizzle_from_expression() {
    return not(bvec2(false, true)).xy;
}

// run: test_bvec2_access_swizzle_from_expression() == bvec2(true, false)

bvec2 test_bvec2_access_swizzle_mixed_names() {
    // Test different name sets (xy, rg, st)
    bvec2 a = bvec2(true, false);
    return a.rg; // Same as xy
}

// run: test_bvec2_access_swizzle_mixed_names() == bvec2(true, false)

bvec2 test_bvec2_access_swizzle_st() {
    bvec2 a = bvec2(true, false);
    return a.st; // Same as xy
}

// run: test_bvec2_access_swizzle_st() == bvec2(true, false)

bvec2 test_bvec2_access_swizzle_in_assignment() {
    bvec2 a = bvec2(true, false);
    bvec2 result = a.yx;
    return result;
}

// run: test_bvec2_access_swizzle_in_assignment() == bvec2(false, true)
