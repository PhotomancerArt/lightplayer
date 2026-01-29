// test run
// target riscv32.q32

// ============================================================================
// From uvec: uvec2(uvec2) - identity constructor, uvec2(uvec3), uvec2(uvec4) - shortening constructors
// ============================================================================

uvec2 test_uvec2_from_uvec2_identity() {
    // Constructor uvec2(uvec2) is identity constructor
    uvec2 source = uvec2(5u, 3u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec2_identity() == uvec2(5u, 3u)

uvec2 test_uvec2_from_uvec2_all_positive() {
    uvec2 source = uvec2(10u, 20u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec2_all_positive() == uvec2(10u, 20u)

uvec2 test_uvec2_from_uvec2_all_zero() {
    uvec2 source = uvec2(0u, 0u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec2_all_zero() == uvec2(0u, 0u)

uvec2 test_uvec2_from_uvec2_max_values() {
    uvec2 source = uvec2(4294967295u, 4294967294u);
    return uvec2(source);
}

// run: test_uvec2_from_uvec2_max_values() == uvec2(4294967295u, 4294967294u)

uvec2 test_uvec2_from_uvec2_variable() {
    uvec2 x = uvec2(42u, 17u);
    return uvec2(x);
}

// run: test_uvec2_from_uvec2_variable() == uvec2(42u, 17u)

uvec2 test_uvec2_from_uvec2_expression() {
    return uvec2(uvec2(10u, 5u) + uvec2(5u, 10u));
}

// run: test_uvec2_from_uvec2_expression() == uvec2(15u, 15u)

uvec2 test_uvec2_from_uvec2_in_assignment() {
    uvec2 source = uvec2(100u, 200u);
    uvec2 result;
    result = uvec2(source);
    return result;
}

// run: test_uvec2_from_uvec2_in_assignment() == uvec2(100u, 200u)

// ============================================================================
// Shortening constructors: uvec2(uvec3), uvec2(uvec4)
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

uvec2 test_uvec2_from_shortening_in_expression() {
    uvec3 source = uvec3(50u, 25u, 75u);
    return uvec2(source) * uvec2(2u, 3u);
}

// run: test_uvec2_from_shortening_in_expression() == uvec2(100u, 75u)
