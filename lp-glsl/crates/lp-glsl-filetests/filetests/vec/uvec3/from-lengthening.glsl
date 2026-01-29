// test run
// target riscv32.q32

// ============================================================================
// From Lengthening: uvec3(uvec2, uint) - lengthening constructor
// ============================================================================

uvec3 test_uvec3_from_uvec2_uint() {
    // Constructor uvec3(uvec2, uint) combines uvec2 and uint
    uvec2 source = uvec2(5u, 3u);
    uint third = 7u;
    return uvec3(source, third);
}

// run: test_uvec3_from_uvec2_uint() == uvec3(5u, 3u, 7u)

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

uvec3 test_uvec3_from_uvec2_uint_max() {
    uvec2 source = uvec2(1u, 2u);
    uint third = 4294967295u;
    return uvec3(source, third);
}

// run: test_uvec3_from_uvec2_uint_max() == uvec3(1u, 2u, 4294967295u)

uvec3 test_uvec3_from_uint_uvec2_mixed() {
    uint first = 42u;
    uvec2 source = uvec2(100u, 200u);
    return uvec3(first, source);
}

// run: test_uvec3_from_uint_uvec2_mixed() == uvec3(42u, 100u, 200u)

uvec3 test_uvec3_from_lengthening_expressions() {
    return uvec3(uvec2(10u, 5u) + uvec2(5u, 10u), 15u);
}

// run: test_uvec3_from_lengthening_expressions() == uvec3(15u, 15u, 15u)

uvec3 test_uvec3_from_lengthening_other_combination() {
    return uvec3(99u, uvec2(50u, 75u) * uvec2(2u, 1u));
}

// run: test_uvec3_from_lengthening_other_combination() == uvec3(99u, 100u, 75u)

uvec3 test_uvec3_from_lengthening_in_assignment() {
    uvec3 result;
    uvec2 source = uvec2(25u, 50u);
    result = uvec3(source, 75u);
    return result;
}

// run: test_uvec3_from_lengthening_in_assignment() == uvec3(25u, 50u, 75u)

uvec3 test_uvec3_from_lengthening_variables() {
    uvec2 vec_part = uvec2(123u, 456u);
    uint scalar_part = 789u;
    return uvec3(vec_part, scalar_part);
}

// run: test_uvec3_from_lengthening_variables() == uvec3(123u, 456u, 789u)
