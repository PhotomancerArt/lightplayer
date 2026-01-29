// test run
// target riscv32.q32

// ============================================================================
// From Mixed: uvec3(int, float, bool, ivec3, bvec3, vec3) - type conversions
// ============================================================================

uvec3 test_uvec3_from_mixed_int_int_int() {
    // Constructor uvec3(int, float, bool) converts values to uint
    return uvec3(42, 3.7, true);
}

// run: test_uvec3_from_mixed_int_int_int() == uvec3(42u, 3u, 1u)

uvec3 test_uvec3_from_mixed_int_float_bool() {
    return uvec3(-5, 2.9, false);
}

// run: test_uvec3_from_mixed_int_float_bool() == uvec3(4294967291u, 2u, 0u)

uvec3 test_uvec3_from_mixed_bool_bool_bool() {
    return uvec3(true, false, true);
}

// run: test_uvec3_from_mixed_bool_bool_bool() == uvec3(1u, 0u, 1u)

uvec3 test_uvec3_from_mixed_ivec3() {
    ivec3 source = ivec3(10, -5, 25);
    return uvec3(source);
}

// run: test_uvec3_from_mixed_ivec3() == uvec3(10u, 4294967291u, 25u)

uvec3 test_uvec3_from_mixed_bvec3() {
    bvec3 source = bvec3(true, false, true);
    return uvec3(source);
}

// run: test_uvec3_from_mixed_bvec3() == uvec3(1u, 0u, 1u)

uvec3 test_uvec3_from_mixed_vec3() {
    vec3 source = vec3(1.5, -2.7, 3.9);
    return uvec3(source);
}

// run: test_uvec3_from_mixed_vec3() == uvec3(1u, 4294967294u, 3u)

uvec3 test_uvec3_from_mixed_zero_values() {
    return uvec3(0, 0.0, false);
}

// run: test_uvec3_from_mixed_zero_values() == uvec3(0u, 0u, 0u)

uvec3 test_uvec3_from_mixed_negative_values() {
    return uvec3(-1, -2.5, -3);
}

// run: test_uvec3_from_mixed_negative_values() == uvec3(4294967295u, 4294967294u, 4294967293u)

uvec3 test_uvec3_from_mixed_large_values() {
    return uvec3(100000, 999.9, true);
}

// run: test_uvec3_from_mixed_large_values() == uvec3(100000u, 999u, 1u)

uvec3 test_uvec3_from_mixed_variables() {
    int x = 100;
    float y = 3.14;
    bool z = true;
    return uvec3(x, y, z);
}

// run: test_uvec3_from_mixed_variables() == uvec3(100u, 3u, 1u)

uvec3 test_uvec3_from_mixed_expressions() {
    return uvec3(10 + 5, 7.8 * 2.0, 5 > 3);
}

// run: test_uvec3_from_mixed_expressions() == uvec3(15u, 15u, 1u)

uvec3 test_uvec3_from_mixed_vector_expressions() {
    return uvec3(ivec3(5, 8, 12) + ivec3(3, 2, 1));
}

// run: test_uvec3_from_mixed_vector_expressions() == uvec3(8u, 10u, 13u)
