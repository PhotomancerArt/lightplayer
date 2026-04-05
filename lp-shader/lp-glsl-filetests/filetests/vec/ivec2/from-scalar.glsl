// test run

// ============================================================================
// From Scalar: ivec2(int) - broadcast single int to all components
// ============================================================================

ivec2 test_ivec2_from_scalar_positive() {
    return ivec2(5);
}

// run: test_ivec2_from_scalar_positive() == ivec2(5, 5)

ivec2 test_ivec2_from_scalar_negative() {
    return ivec2(-3);
}

// run: test_ivec2_from_scalar_negative() == ivec2(-3, -3)

ivec2 test_ivec2_from_scalar_zero() {
    return ivec2(0);
}

// run: test_ivec2_from_scalar_zero() == ivec2(0, 0)

ivec2 test_ivec2_from_scalar_variable() {
    int x = 42;
    return ivec2(x);
}

// run: test_ivec2_from_scalar_variable() == ivec2(42, 42)

ivec2 test_ivec2_from_scalar_expression() {
    return ivec2(10 - 5);
}

// run: test_ivec2_from_scalar_expression() == ivec2(5, 5)

ivec2 test_ivec2_from_scalar_function_result() {
    return ivec2(int(7.8));
}

// run: test_ivec2_from_scalar_function_result() == ivec2(7, 7)

ivec2 test_ivec2_from_scalar_in_assignment() {
    ivec2 result;
    result = ivec2(-8);
    return result;
}

// run: test_ivec2_from_scalar_in_assignment() == ivec2(-8, -8)

ivec2 test_ivec2_from_scalar_large_value() {
    return ivec2(100000);
}

// run: test_ivec2_from_scalar_large_value() == ivec2(100000, 100000)

ivec2 test_ivec2_from_scalar_computation() {
    return ivec2(2 * 3 + 1);
}

// run: test_ivec2_from_scalar_computation() == ivec2(7, 7)

// ----------------------------------------------------------------------------
// Call-argument stack (WASM): ivec2(scalar) must contribute exactly 2 values
// before the next argument (lp-glsl-wasm broadcast / multi-arg calls).
// ----------------------------------------------------------------------------

int ivec2_broadcast_sum_args(ivec2 a, ivec2 b) {
    return a.x + a.y + b.x + b.y;
}

int test_ivec2_from_scalar_as_first_call_arg() {
    return ivec2_broadcast_sum_args(ivec2(2), ivec2(10, 20));
}

// run: test_ivec2_from_scalar_as_first_call_arg() == 34
