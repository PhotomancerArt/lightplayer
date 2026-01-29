// test run
// target riscv32.q32

// ============================================================================
// From Mixed: uvec2(int, float, bool, ivec2, bvec2, vec2) - type conversions
// ============================================================================

uvec2 test_uvec2_from_mixed_int_int() {
    // Constructor uvec2(int, float) converts numeric values to uint (truncates)
    return uvec2(42, 3.7);
}

// run: test_uvec2_from_mixed_int_int() == uvec2(42u, 3u)

uvec2 test_uvec2_from_mixed_int_float() {
    return uvec2(-5, 2.9);
}

// run: test_uvec2_from_mixed_int_float() == uvec2(4294967291u, 2u)

uvec2 test_uvec2_from_mixed_bool_bool() {
    return uvec2(true, false);
}

// run: test_uvec2_from_mixed_bool_bool() == uvec2(1u, 0u)

uvec2 test_uvec2_from_mixed_bool_false() {
    return uvec2(false, true);
}

// run: test_uvec2_from_mixed_bool_false() == uvec2(0u, 1u)

uvec2 test_uvec2_from_mixed_ivec2() {
    ivec2 source = ivec2(10, -5);
    return uvec2(source);
}

// run: test_uvec2_from_mixed_ivec2() == uvec2(10u, 4294967291u)

uvec2 test_uvec2_from_mixed_bvec2() {
    bvec2 source = bvec2(true, false);
    return uvec2(source);
}

// run: test_uvec2_from_mixed_bvec2() == uvec2(1u, 0u)

uvec2 test_uvec2_from_mixed_vec2() {
    vec2 source = vec2(1.5, -2.7);
    return uvec2(source);
}

// run: test_uvec2_from_mixed_vec2() == uvec2(1u, 4294967294u)

uvec2 test_uvec2_from_mixed_zero_values() {
    return uvec2(0, 0.0);
}

// run: test_uvec2_from_mixed_zero_values() == uvec2(0u, 0u)

uvec2 test_uvec2_from_mixed_negative_values() {
    return uvec2(-1, -2.5);
}

// run: test_uvec2_from_mixed_negative_values() == uvec2(4294967295u, 4294967294u)

uvec2 test_uvec2_from_mixed_variables() {
    int x = 100;
    float y = 3.14;
    bool z = true;
    return uvec2(x, y);
}

// run: test_uvec2_from_mixed_variables() == uvec2(100u, 3u)

uvec2 test_uvec2_from_mixed_expressions() {
    return uvec2(10 + 5, 7.8 * 2.0);
}

// run: test_uvec2_from_mixed_expressions() == uvec2(15u, 15u)

uvec2 test_uvec2_from_mixed_vector_expressions() {
    return uvec2(ivec2(5, 8) + ivec2(3, 2));
}

// run: test_uvec2_from_mixed_vector_expressions() == uvec2(8u, 10u)
