// test run
// target riscv32.q32

// ============================================================================
// From uvec: uvec3(uvec3) - identity constructor, uvec3(uvec4) - shortening constructor, uvec3(uvec2, uint) - lengthening constructor
// ============================================================================

uvec3 test_uvec3_from_uvec3_identity() {
    // Constructor uvec3(uvec3) is identity constructor
    uvec3 source = uvec3(5u, 3u, 7u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec3_identity() == uvec3(5u, 3u, 7u)

uvec3 test_uvec3_from_uvec3_all_positive() {
    uvec3 source = uvec3(10u, 20u, 30u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec3_all_positive() == uvec3(10u, 20u, 30u)

uvec3 test_uvec3_from_uvec3_all_zero() {
    uvec3 source = uvec3(0u, 0u, 0u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec3_all_zero() == uvec3(0u, 0u, 0u)

uvec3 test_uvec3_from_uvec3_max_values() {
    uvec3 source = uvec3(4294967295u, 4294967294u, 4294967293u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec3_max_values() == uvec3(4294967295u, 4294967294u, 4294967293u)

uvec3 test_uvec3_from_uvec3_variable() {
    uvec3 x = uvec3(42u, 17u, 99u);
    return uvec3(x);
}

// run: test_uvec3_from_uvec3_variable() == uvec3(42u, 17u, 99u)

uvec3 test_uvec3_from_uvec3_expression() {
    return uvec3(uvec3(10u, 5u, 15u) + uvec3(5u, 10u, 10u));
}

// run: test_uvec3_from_uvec3_expression() == uvec3(15u, 15u, 25u)

uvec3 test_uvec3_from_uvec3_in_assignment() {
    uvec3 source = uvec3(100u, 200u, 300u);
    uvec3 result;
    result = uvec3(source);
    return result;
}

// run: test_uvec3_from_uvec3_in_assignment() == uvec3(100u, 200u, 300u)

// ============================================================================
// Shortening constructor: uvec3(uvec4)
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

uvec3 test_uvec3_from_uvec4_mixed() {
    uvec4 source = uvec4(123u, 456u, 789u, 999u);
    return uvec3(source);
}

// run: test_uvec3_from_uvec4_mixed() == uvec3(123u, 456u, 789u)

uvec3 test_uvec3_from_uvec4_in_expression() {
    uvec4 source = uvec4(50u, 25u, 75u, 100u);
    return uvec3(source) * uvec3(2u, 3u, 1u);
}

// run: test_uvec3_from_uvec4_in_expression() == uvec3(100u, 75u, 75u)

// ============================================================================
// Lengthening constructor: uvec3(uvec2, uint)
// ============================================================================

uvec3 test_uvec3_from_uvec2_uint() {
    // Constructor uvec3(uvec2, uint) combines uvec2 and uint
    uvec2 source = uvec2(5u, 3u);
    uint third = 7u;
    return uvec3(source, third);
}

// run: test_uvec3_from_uvec3_from_uvec2_uint() == uvec3(5u, 3u, 7u) [expect-fail]

uvec3 test_uvec3_from_uint_uvec2() {
    // Constructor uvec3(uint, uvec2) combines uint and uvec2
    uint first = 2u;
    uvec2 source = uvec2(10u, 20u);
    return uvec3(first, source);
}

// run: test_uvec3_from_uint_uvec2() == uvec3(2u, 10u, 20u)

uvec3 test_uvec3_from_uvec2_uint_zero() {
    uvec2 source = uvec2(100u, 200u);
    uint third = 0u;
    return uvec3(source, third);
}

// run: test_uvec3_from_uvec2_uint_zero() == uvec3(100u, 200u, 0u)

uvec3 test_uvec3_from_uint_uvec2_max() {
    uint first = 4294967295u;
    uvec2 source = uvec2(1u, 2u);
    return uvec3(first, source);
}

// run: test_uvec3_from_uint_uvec2_max() == uvec3(4294967295u, 1u, 2u)

uvec3 test_uvec3_from_lengthening_expressions() {
    return uvec3(uvec2(10u, 5u) + uvec2(5u, 10u), 15u);
}

// run: test_uvec3_from_lengthening_expressions() == uvec3(15u, 15u, 15u)

uvec3 test_uvec3_from_lengthening_other_combination() {
    return uvec3(42u, uvec2(100u, 200u) * uvec2(2u, 1u));
}

// run: test_uvec3_from_lengthening_other_combination() == uvec3(42u, 200u, 200u)

uvec3 test_uvec3_from_lengthening_in_assignment() {
    uvec3 result;
    uvec2 source = uvec2(50u, 75u);
    result = uvec3(source, 25u);
    return result;
}

// run: test_uvec3_from_lengthening_in_assignment() == uvec3(50u, 75u, 25u)
