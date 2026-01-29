// test run
// target riscv32.q32

// ============================================================================
// Access Component: bvec2.x, bvec2.y - component name access
// ============================================================================

bool test_bvec2_access_component_x() {
    // Component name access
    bvec2 a = bvec2(true, false);
    return a.x;
}

// run: test_bvec2_access_component_x() == true

bool test_bvec2_access_component_y() {
    bvec2 a = bvec2(true, false);
    return a.y;
}

// run: test_bvec2_access_component_y() == false

bool test_bvec2_access_component_x_from_expression() {
    return not(bvec2(false, true)).x;
}

// run: test_bvec2_access_component_x_from_expression() == true

bool test_bvec2_access_component_y_from_expression() {
    return not(bvec2(true, false)).y;
}

// run: test_bvec2_access_component_y_from_expression() == true

bool test_bvec2_access_component_in_assignment() {
    bvec2 a = bvec2(true, false);
    bool result = a.x;
    return result;
}

// run: test_bvec2_access_component_in_assignment() == true

bool test_bvec2_access_component_both() {
    bvec2 a = bvec2(true, false);
    bool result = a.x && !a.y;
    return result;
}

// run: test_bvec2_access_component_both() == true

bool test_bvec2_access_component_mixed_names() {
    // Test different name sets (xy, rg, st)
    bvec2 a = bvec2(true, false);
    return a.r; // Same as x
}

// run: test_bvec2_access_component_mixed_names() == true
