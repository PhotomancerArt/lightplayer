// test run
// target riscv32.q32

// ============================================================================
// Assign Simple: bvec2 = bvec2
// ============================================================================

bvec2 test_bvec2_assign_simple() {
    // Simple assignment
    bvec2 a = bvec2(true, false);
    bvec2 b = a;
    return b;
}

// run: test_bvec2_assign_simple() == bvec2(true, false)

bvec2 test_bvec2_assign_simple_independence() {
    // Verify independence (modifying one doesn't affect the other)
    bvec2 a = bvec2(true, false);
    bvec2 b = a;
    b = bvec2(false, true);
    return a; // Should still be original value
}

// run: test_bvec2_assign_simple_independence() == bvec2(true, false)

bvec2 test_bvec2_assign_simple_self() {
    // Self-assignment
    bvec2 a = bvec2(true, false);
    a = a;
    return a;
}

// run: test_bvec2_assign_simple_self() == bvec2(true, false)

bvec2 test_bvec2_assign_simple_chain() {
    // Chain assignment
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    bvec2 c = bvec2(true, true);
    c = b = a;
    return c;
}

// run: test_bvec2_assign_simple_chain() == bvec2(true, false)

bvec2 test_bvec2_assign_simple_from_expression() {
    bvec2 result;
    result = not(bvec2(false, true));
    return result;
}

// run: test_bvec2_assign_simple_from_expression() == bvec2(true, false)

bvec2 test_bvec2_assign_simple_in_declaration() {
    bvec2 result = bvec2(true, false);
    return result;
}

// run: test_bvec2_assign_simple_in_declaration() == bvec2(true, false)
