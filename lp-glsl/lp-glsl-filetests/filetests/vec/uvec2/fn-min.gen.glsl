// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec2/fn-min --write
//
// test run
// target riscv32.q32

// ============================================================================
// Min: min(uvec2, uvec2) -> uvec2 (component-wise minimum)
// ============================================================================

uvec2 test_uvec2_min_first_smaller() {
    // Function min() returns uvec2 (component-wise minimum)
    uvec2 a = uvec2(3u, 8u);
    uvec2 b = uvec2(7u, 4u);
    return min(a, b);
}

// run: test_uvec2_min_first_smaller() == uvec2(3u, 4u)

uvec2 test_uvec2_min_second_smaller() {
    uvec2 a = uvec2(7u, 8u);
    uvec2 b = uvec2(3u, 4u);
    return min(a, b);
}

// run: test_uvec2_min_second_smaller() == uvec2(3u, 4u)

uvec2 test_uvec2_min_mixed() {
    uvec2 a = uvec2(3u, 8u);
    uvec2 b = uvec2(7u, 4u);
    return min(a, b);
}

// run: test_uvec2_min_mixed() == uvec2(3u, 4u)

uvec2 test_uvec2_min_equal() {
    uvec2 a = uvec2(5u, 5u);
    uvec2 b = uvec2(5u, 5u);
    return min(a, b);
}

// run: test_uvec2_min_equal() == uvec2(5u, 5u)

uvec2 test_uvec2_min_zero() {
    uvec2 a = uvec2(0u, 5u);
    uvec2 b = uvec2(2u, 3u);
    return min(a, b);
}

// run: test_uvec2_min_zero() == uvec2(0u, 3u)

uvec2 test_uvec2_min_variables() {
    uvec2 a = uvec2(10u, 15u);
    uvec2 b = uvec2(12u, 10u);
    return min(a, b);
}

// run: test_uvec2_min_variables() == uvec2(10u, 10u)

uvec2 test_uvec2_min_expressions() {
    return min(uvec2(6u, 2u), uvec2(4u, 7u));
}

// run: test_uvec2_min_expressions() == uvec2(4u, 2u)

uvec2 test_uvec2_min_in_expression() {
    uvec2 a = uvec2(3u, 8u);
    uvec2 b = uvec2(7u, 4u);
    uvec2 c = uvec2(1u, 6u);
    return min(a, min(b, c));
}

// run: test_uvec2_min_in_expression() == uvec2(1u, 4u)
