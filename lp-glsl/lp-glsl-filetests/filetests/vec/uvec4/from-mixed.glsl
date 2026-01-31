// test run
// target riscv32.q32

// ============================================================================
// From Mixed: uvec4(int, float, bool, ivec4, bvec4, vec4) - type conversions
// ============================================================================

uvec4 test_uvec4_from_mixed_int_int_int_int() {
    // Constructor uvec4(int, float, bool, int) converts values to uint
    return uvec4(42, 3.7, true, -8);
}

// run: test_uvec4_from_mixed_int_int_int_int() == uvec4(42u, 3u, 1u, 4294967288u)

uvec4 test_uvec4_from_mixed_int_float_bool_int() {
    return uvec4(-5, 2.9, false, 100);
}

// run: test_uvec4_from_mixed_int_float_bool_int() == uvec4(4294967291u, 2u, 0u, 100u)

uvec4 test_uvec4_from_mixed_bool_bool_bool_bool() {
    return uvec4(true, false, true, false);
}

// run: test_uvec4_from_mixed_bool_bool_bool_bool() == uvec4(1u, 0u, 1u, 0u)

uvec4 test_uvec4_from_mixed_ivec4() {
    ivec4 source = ivec4(10, -5, 25, -15);
    return uvec4(source);
}

// run: test_uvec4_from_mixed_ivec4() == uvec4(10u, 4294967291u, 25u, 4294967281u)

uvec4 test_uvec4_from_mixed_bvec4() {
    bvec4 source = bvec4(true, false, true, false);
    return uvec4(source);
}

// run: test_uvec4_from_mixed_bvec4() == uvec4(1u, 0u, 1u, 0u)

uvec4 test_uvec4_from_mixed_vec4() {
    vec4 source = vec4(1.5, -2.7, 3.9, 0.0);
    return uvec4(source);
}

// run: test_uvec4_from_mixed_vec4() == uvec4(1u, 4294967294u, 3u, 0u)

uvec4 test_uvec4_from_mixed_zero_values() {
    return uvec4(0, 0.0, false, 0);
}

// run: test_uvec4_from_mixed_zero_values() == uvec4(0u, 0u, 0u, 0u)

uvec4 test_uvec4_from_mixed_negative_values() {
    return uvec4(-1, -2.5, -3, -4.9);
}

// run: test_uvec4_from_mixed_negative_values() == uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u)

uvec4 test_uvec4_from_mixed_large_values() {
    return uvec4(100000, 999.9, true, 50000);
}

// run: test_uvec4_from_mixed_large_values() == uvec4(100000u, 999u, 1u, 50000u)

uvec4 test_uvec4_from_mixed_variables() {
    int x = 100;
    float y = 3.14;
    bool z = true;
    int w = -50;
    return uvec4(x, y, z, w);
}

// run: test_uvec4_from_mixed_variables() == uvec4(100u, 3u, 1u, 4294967246u)

uvec4 test_uvec4_from_mixed_expressions() {
    return uvec4(10 + 5, 7.8 * 2.0, 5 > 3, -10 + 20);
}

// run: test_uvec4_from_mixed_expressions() == uvec4(15u, 15u, 1u, 10u)

uvec4 test_uvec4_from_mixed_vector_expressions() {
    return uvec4(ivec4(5, 8, 12, 3) + ivec4(3, 2, 1, 7));
}

// run: test_uvec4_from_mixed_vector_expressions() == uvec4(8u, 10u, 13u, 10u)
