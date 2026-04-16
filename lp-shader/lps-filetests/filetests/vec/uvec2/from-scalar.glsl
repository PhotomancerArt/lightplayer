// test run

// ============================================================================
// From Scalar: uvec2(uint) - broadcast single uint to all components
// ============================================================================

uvec2 test_uvec2_from_scalar_positive() {
    // Constructor uvec2(uint) broadcasts single uint to all components
    return uvec2(5u);
}

// run: test_uvec2_from_scalar_positive() == uvec2(5u, 5u)

uvec2 test_uvec2_from_scalar_zero() {
    return uvec2(0u);
}

// run: test_uvec2_from_scalar_zero() == uvec2(0u, 0u)

uvec2 test_uvec2_from_scalar_max() {
    return uvec2(4294967295u);
}

// run: test_uvec2_from_scalar_max() == uvec2(4294967295u, 4294967295u)

uvec2 test_uvec2_from_scalar_variable() {
    uint x = 42u;
    return uvec2(x);
}

// run: test_uvec2_from_scalar_variable() == uvec2(42u, 42u)

uvec2 test_uvec2_from_scalar_expression() {
    return uvec2(10u - 5u);
}

// run: test_uvec2_from_scalar_expression() == uvec2(5u, 5u)

uvec2 test_uvec2_from_scalar_function_result() {
    return uvec2(uint(7.8)); // float to uint conversion (truncates)
}

// run: test_uvec2_from_scalar_function_result() == uvec2(7u, 7u)

uvec2 test_uvec2_from_scalar_in_assignment() {
    uvec2 result;
    result = uvec2(8u);
    return result;
}

// run: test_uvec2_from_scalar_in_assignment() == uvec2(8u, 8u)

uvec2 test_uvec2_from_scalar_large_value() {
    return uvec2(100000u);
}

// run: test_uvec2_from_scalar_large_value() == uvec2(100000u, 100000u)

uvec2 test_uvec2_from_scalar_computation() {
    return uvec2(2u * 3u + 1u);
}

// run: test_uvec2_from_scalar_computation() == uvec2(7u, 7u)

// ----------------------------------------------------------------------------
// Call-argument stack (WASM): uvec2(scalar) must contribute exactly 2 values
// before the next argument (lps-wasm broadcast / multi-arg calls).
// ----------------------------------------------------------------------------

uint uvec2_broadcast_sum_args(uvec2 a, uvec2 b) {
    return a.x + a.y + b.x + b.y;
}

uint test_uvec2_from_scalar_as_first_call_arg() {
    return uvec2_broadcast_sum_args(uvec2(2u), uvec2(10u, 20u));
}

// run: test_uvec2_from_scalar_as_first_call_arg() == 34u
