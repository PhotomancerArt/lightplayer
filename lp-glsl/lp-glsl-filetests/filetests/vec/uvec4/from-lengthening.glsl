// test run
// target riscv32.q32

// ============================================================================
// From Lengthening: uvec4(uvec2, uint, uint), uvec4(uvec3, uint) - lengthening constructors
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

uvec4 test_uvec4_from_lengthening_variables() {
    uvec2 vec_part = uvec2(123u, 456u);
    uint scalar1 = 789u;
    uint scalar2 = 999u;
    return uvec4(vec_part, scalar1, scalar2);
}

// run: test_uvec4_from_lengthening_variables() == uvec4(123u, 456u, 789u, 999u)

uvec4 test_uvec4_from_uvec3_uint_variables() {
    uvec3 vec_part = uvec3(111u, 222u, 333u);
    uint scalar_part = 444u;
    return uvec4(vec_part, scalar_part);
}

// run: test_uvec4_from_uvec3_uint_variables() == uvec4(111u, 222u, 333u, 444u)
