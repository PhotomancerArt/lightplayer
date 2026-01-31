// test run
// target riscv32.q32

// ============================================================================
// Assign Swizzle: bvec4.xyzw = bvec4 - multi-component swizzle assignment
// ============================================================================

bvec4 test_bvec4_assign_swizzle_xyzw_full() {
    // Assign to full swizzle
    bvec4 a = bvec4(false, false, false, false);
    bvec4 source = bvec4(true, true, true, true);
    a.xyzw = source;
    return a;
}

// run: test_bvec4_assign_swizzle_xyzw_full() == bvec4(true, true, true, true)

bvec4 test_bvec4_assign_swizzle_xyzw_partial() {
    bvec4 a = bvec4(true, false, true, false);
    bvec4 source = bvec4(false, true, false, true);
    a.xyzw = source;
    return a;
}

// run: test_bvec4_assign_swizzle_xyzw_partial() == bvec4(false, true, false, true)

bvec4 test_bvec4_assign_swizzle_wzyx() {
    // Reverse swizzle assignment
    bvec4 a = bvec4(false, false, false, false);
    bvec4 source = bvec4(true, false, true, false);
    a.wzyx = source;
    return a;
}

// run: test_bvec4_assign_swizzle_wzyx() == bvec4(false, true, false, true)

bvec4 test_bvec4_assign_swizzle_from_expression() {
    bvec4 a = bvec4(false, false, false, false);
    a.xyzw = not(bvec4(false, true, false, true));
    return a;
}

// run: test_bvec4_assign_swizzle_from_expression() == bvec4(true, false, true, false)

bvec4 test_bvec4_assign_swizzle_mixed_names() {
    // Test different name sets (xyzw, rgba, stpq)
    bvec4 a = bvec4(false, false, false, false);
    bvec4 source = bvec4(true, false, true, false);
    a.rgba = source; // Same as xyzw
    return a;
}

// run: test_bvec4_assign_swizzle_mixed_names() == bvec4(true, false, true, false)

bvec4 test_bvec4_assign_swizzle_xy() {
    // Partial swizzle assignment (first two components)
    bvec4 a = bvec4(false, false, true, true);
    bvec2 source = bvec2(true, false);
    a.xy = source;
    return a;
}

// run: test_bvec4_assign_swizzle_xy() == bvec4(true, false, true, true)

bvec4 test_bvec4_assign_swizzle_xyz() {
    // Partial swizzle assignment (first three components)
    bvec4 a = bvec4(false, false, false, true);
    bvec3 source = bvec3(true, false, true);
    a.xyz = source;
    return a;
}

// run: test_bvec4_assign_swizzle_xyz() == bvec4(true, false, true, true)

bvec4 main() {
    return test_bvec4_assign_swizzle_xyz();
}

