// test run
// target riscv32.q32

// ============================================================================
// From Shortening: uvec3(uvec4) - shortening constructor
// ============================================================================

uvec3 test_uvec3_from_uvec4() {
    // Constructor uvec3(uvec4) extracts first three components
    uvec4 source = uvec4(5u, 3u, 7u, 2u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec4() == uvec3(5u, 3u, 7u)

uvec3 test_uvec3_from_uvec4_all_positive() {
    uvec4 source = uvec4(10u, 20u, 30u, 40u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec4_all_positive() == uvec3(10u, 20u, 30u)

uvec3 test_uvec3_from_uvec4_all_zero() {
    uvec4 source = uvec4(0u, 0u, 0u, 0u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec4_all_zero() == uvec3(0u, 0u, 0u)

uvec3 test_uvec3_from_uvec4_mixed() {
    uvec4 source = uvec4(123u, 456u, 789u, 999u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec4_mixed() == uvec3(123u, 456u, 789u)

uvec3 test_uvec3_from_uvec4_max_values() {
    uvec4 source = uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec4_max_values() == uvec3(4294967295u, 4294967294u, 4294967293u)

uvec3 test_uvec3_from_uvec4_partial_max() {
    uvec4 source = uvec4(4294967295u, 4294967294u, 4294967293u, 1000u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec4_partial_max() == uvec3(4294967295u, 4294967294u, 4294967293u)

uvec3 test_uvec3_from_shortening_in_expression() {
    uvec4 source = uvec4(50u, 25u, 75u, 100u);
    return uvec3(source) * uvec3(2u, 3u, 1u);
}

// run: test_uvec3_from_shortening_in_expression() == uvec3(100u, 75u, 75u)

uvec3 test_uvec3_from_uvec4_variable() {
    uvec4 source = uvec4(12u, 34u, 56u, 78u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec4_variable() == uvec3(12u, 34u, 56u)
