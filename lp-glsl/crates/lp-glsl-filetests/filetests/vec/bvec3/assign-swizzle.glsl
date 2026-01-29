// test run
// target riscv32.q32

// ============================================================================
// Assign Swizzle: bvec3.xyz = bvec3 - multi-component swizzle assignment
// ============================================================================

bvec3 test_bvec3_assign_swizzle_xyz_full() {
    // Assign to full swizzle
    bvec3 a = bvec3(false, false, false);
    bvec3 source = bvec3(true, true, true);
    a.xyz = source;
    return a;
}

// run: test_bvec3_assign_swizzle_xyz_full() == bvec3(true, true, true)

bvec3 test_bvec3_assign_swizzle_xyz_partial() {
    bvec3 a = bvec3(true, false, true);
    bvec3 source = bvec3(false, true, false);
    a.xyz = source;
    return a;
}

// run: test_bvec3_assign_swizzle_xyz_partial() == bvec3(false, true, false)

bvec3 test_bvec3_assign_swizzle_zyx() {
    // Reverse swizzle assignment
    bvec3 a = bvec3(false, false, false);
    bvec3 source = bvec3(true, false, true);
    a.zyx = source;
    return a;
}

// run: test_bvec3_assign_swizzle_zyx() == bvec3(true, false, true)

bvec3 test_bvec3_assign_swizzle_from_expression() {
    bvec3 a = bvec3(false, false, false);
    a.xyz = not(bvec3(false, true, false));
    return a;
}

// run: test_bvec3_assign_swizzle_from_expression() == bvec3(true, false, true)

bvec3 test_bvec3_assign_swizzle_mixed_names() {
    // Test different name sets (xyz, rgb, stp)
    bvec3 a = bvec3(false, false, false);
    bvec3 source = bvec3(true, false, true);
    a.rgb = source; // Same as xyz
    return a;
}

// run: test_bvec3_assign_swizzle_mixed_names() == bvec3(true, false, true)

bvec3 test_bvec3_assign_swizzle_xy() {
    // Partial swizzle assignment (first two components)
    bvec3 a = bvec3(false, false, true);
    bvec2 source = bvec2(true, false);
    a.xy = source;
    return a;
}

// run: test_bvec3_assign_swizzle_xy() == bvec3(true, false, true)

bvec3 main() {
    return test_bvec3_assign_swizzle_xy();
}

