// test run
// target riscv32.q32

// ============================================================================
// From Shortening: uvec2(uvec3), uvec2(uvec4) - shortening constructors
// ============================================================================

uvec2 test_uvec2_from_uvec3() {
    // Constructor uvec2(uvec3) extracts first two components
    uvec3 source = uvec3(5u, 3u, 7u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec3() == uvec2(5u, 3u)

uvec2 test_uvec2_from_uvec4() {
    // Constructor uvec2(uvec4) extracts first two components
    uvec4 source = uvec4(5u, 3u, 7u, 2u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec4() == uvec2(5u, 3u)

uvec2 test_uvec2_from_uvec3_all_positive() {
    uvec3 source = uvec3(10u, 20u, 30u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec3_all_positive() == uvec2(10u, 20u)

uvec2 test_uvec2_from_uvec3_all_zero() {
    uvec3 source = uvec3(0u, 0u, 0u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec3_all_zero() == uvec2(0u, 0u)

uvec2 test_uvec2_from_uvec4_mixed() {
    uvec4 source = uvec4(123u, 456u, 789u, 999u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec4_mixed() == uvec2(123u, 456u)

uvec2 test_uvec2_from_uvec3_max_values() {
    uvec3 source = uvec3(4294967295u, 4294967294u, 4294967293u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec3_max_values() == uvec2(4294967295u, 4294967294u)

uvec2 test_uvec2_from_uvec4_max_values() {
    uvec4 source = uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec4_max_values() == uvec2(4294967295u, 4294967294u)

uvec2 test_uvec2_from_shortening_in_expression() {
    uvec3 source = uvec3(50u, 25u, 75u);
    return uvec2(source) * uvec2(2u, 3u);
}

// run: test_uvec2_from_shortening_in_expression() == uvec2(100u, 75u)

uvec2 test_uvec2_from_uvec4_in_expression() {
    uvec4 source = uvec4(100u, 50u, 25u, 12u);
    return uvec2(source) + uvec2(10u, 20u);
}

// run: test_uvec2_from_uvec4_in_expression() == uvec2(110u, 70u)
