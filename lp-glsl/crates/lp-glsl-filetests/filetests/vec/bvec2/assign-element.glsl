// test run
// target riscv32.q32

// ============================================================================
// Assign Element: bvec2.x = bool, bvec2[0] = bool - single component assignment
// ============================================================================

bvec2 test_bvec2_assign_element_x() {
    // Assign to single component by name
    bvec2 a = bvec2(false, false);
    a.x = true;
    return a;
}

// run: test_bvec2_assign_element_x() == bvec2(true, false)

bvec2 test_bvec2_assign_element_y() {
    bvec2 a = bvec2(false, false);
    a.y = true;
    return a;
}

// run: test_bvec2_assign_element_y() == bvec2(false, true)

bvec2 test_bvec2_assign_element_index_0() {
    // Assign to single component by index
    bvec2 a = bvec2(false, false);
    a[0] = true;
    return a;
}

// run: test_bvec2_assign_element_index_0() == bvec2(true, false)

bvec2 test_bvec2_assign_element_index_1() {
    bvec2 a = bvec2(false, false);
    a[1] = true;
    return a;
}

// run: test_bvec2_assign_element_index_1() == bvec2(false, true)

bvec2 test_bvec2_assign_element_overwrite() {
    // Verify other components unchanged
    bvec2 a = bvec2(true, true);
    a.x = false;
    return a;
}

// run: test_bvec2_assign_element_overwrite() == bvec2(false, true)

bvec2 test_bvec2_assign_element_from_expression() {
    bvec2 a = bvec2(false, false);
    a.y = any(bvec2(true, false));
    return a;
}

// run: test_bvec2_assign_element_from_expression() == bvec2(false, true)

bvec2 test_bvec2_assign_element_both() {
    // Assign to both components
    bvec2 a = bvec2(false, false);
    a.x = true;
    a.y = false;
    return a;
}

// run: test_bvec2_assign_element_both() == bvec2(true, false)
