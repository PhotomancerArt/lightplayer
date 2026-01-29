// test run
// target riscv32.q32

// ============================================================================
// From uvec: uvec4(uvec4) - identity constructor, uvec4(uvec2, uint, uint), uvec4(uvec3, uint) - lengthening constructors
// ============================================================================

uvec4 test_uvec4_from_uvec4_identity() {
    // Constructor uvec4(uvec4) is identity constructor
    uvec4 source = uvec4(5u, 3u, 7u, 2u);
    return uvec4(source);
}

// run: test_uvec4_from_uvec4_identity() == uvec4(5u, 3u, 7u, 2u)

uvec4 test_uvec4_from_uvec4_all_positive() {
    uvec4 source = uvec4(10u, 20u, 30u, 40u);
    return uvec4(source);
}

// run: test_uvec4_from_uvec4_all_positive() == uvec4(10u, 20u, 30u, 40u)

uvec4 test_uvec4_from_uvec4_all_zero() {
    uvec4 source = uvec4(0u, 0u, 0u, 0u);
    return uvec4(source);
}

// run: test_uvec4_from_uvec4_all_zero() == uvec4(0u, 0u, 0u, 0u)

uvec4 test_uvec4_from_uvec4_max_values() {
    uvec4 source = uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u);
    return uvec4(source);
}

// run: test_uvec4_from_uvec4_max_values() == uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u)

uvec4 test_uvec4_from_uvec4_variable() {
    uvec4 x = uvec4(42u, 17u, 99u, 55u);
    return uvec4(x);
}

// run: test_uvec4_from_uvec4_variable() == uvec4(42u, 17u, 99u, 55u)

uvec4 test_uvec4_from_uvec4_expression() {
    return uvec4(uvec4(10u, 5u, 15u, 8u) + uvec4(5u, 10u, 10u, 12u));
}

// run: test_uvec4_from_uvec4_expression() == uvec4(15u, 15u, 25u, 20u)

uvec4 test_uvec4_from_uvec4_in_assignment() {
    uvec4 source = uvec4(100u, 200u, 300u, 400u);
    uvec4 result;
    result = uvec4(source);
    return result;
}

// run: test_uvec4_from_uvec4_in_assignment() == uvec4(100u, 200u, 300u, 400u)

// ============================================================================
// Lengthening constructors: uvec4(uvec2, uint, uint), uvec4(uvec3, uint)
// ============================================================================

uvec4 test_uvec4_from_uvec2_uint_uint() {
    // Constructor uvec4(uvec2, uint, uint) combines uvec2 and two uints
    uvec2 source = uvec2(5u, 3u);
    uint third = 7u;
    uint fourth = 2u;
    return uvec4(source, third, fourth);
}

// run: test_uvec4_from_uvec2_uint_uint() == uvec4(5u, 3u, 7u, 2u)

uvec4 test_uvec4_from_uint_uvec2_uint() {
    // Constructor uvec4(uint, uvec2, uint) combines uint, uvec2, and uint
    uint first = 1u;
    uvec2 source = uvec2(10u, 20u);
    uint fourth = 4u;
    return uvec4(first, source, fourth);
}

// run: test_uvec4_from_uint_uvec2_uint() == uvec4(1u, 10u, 20u, 4u)

uvec4 test_uvec4_from_uint_uint_uvec2() {
    // Constructor uvec4(uint, uint, uvec2) combines two uints and uvec2
    uint first = 8u;
    uint second = 6u;
    uvec2 source = uvec2(12u, 14u);
    return uvec4(first, second, source);
}

// run: test_uvec4_from_uint_uint_uvec2() == uvec4(8u, 6u, 12u, 14u)

uvec4 test_uvec4_from_uvec3_uint() {
    // Constructor uvec4(uvec3, uint) combines uvec3 and uint
    uvec3 source = uvec3(5u, 3u, 7u);
    uint fourth = 9u;
    return uvec4(source, fourth);
}

// run: test_uvec4_from_uvec3_uint() == uvec4(5u, 3u, 7u, 9u)

uvec4 test_uvec4_from_uint_uvec3() {
    // Constructor uvec4(uint, uvec3) combines uint and uvec3
    uint first = 11u;
    uvec3 source = uvec3(13u, 15u, 17u);
    return uvec4(first, source);
}

// run: test_uvec4_from_uint_uvec3() == uvec4(11u, 13u, 15u, 17u)

uvec4 test_uvec4_from_uvec2_uint_uint_zero() {
    uvec2 source = uvec2(100u, 200u);
    uint third = 0u;
    uint fourth = 0u;
    return uvec4(source, third, fourth);
}

// run: test_uvec4_from_uvec2_uint_uint_zero() == uvec4(100u, 200u, 0u, 0u)

uvec4 test_uvec4_from_uvec3_uint_max() {
    uvec3 source = uvec3(1u, 2u, 3u);
    uint fourth = 4294967295u;
    return uvec4(source, fourth);
}

// run: test_uvec4_from_uvec3_uint_max() == uvec4(1u, 2u, 3u, 4294967295u)

uvec4 test_uvec4_from_lengthening_expressions() {
    return uvec4(uvec2(10u, 5u) + uvec2(5u, 10u), 15u, 25u);
}

// run: test_uvec4_from_lengthening_expressions() == uvec4(15u, 15u, 15u, 25u)

uvec4 test_uvec4_from_lengthening_other_combination() {
    return uvec4(42u, uvec2(100u, 200u) * uvec2(2u, 1u), 99u);
}

// run: test_uvec4_from_lengthening_other_combination() == uvec4(42u, 200u, 200u, 99u)

uvec4 test_uvec4_from_uvec3_uint_in_assignment() {
    uvec4 result;
    uvec3 source = uvec3(50u, 75u, 25u);
    result = uvec4(source, 100u);
    return result;
}

// run: test_uvec4_from_uvec3_uint_in_assignment() == uvec4(50u, 75u, 25u, 100u)
