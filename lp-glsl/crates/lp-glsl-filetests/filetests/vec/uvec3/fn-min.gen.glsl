// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec3/fn-min --write
//
// test run
// target riscv32.q32

// ============================================================================
// Min: min(uvec3, uvec3) -> uvec3 (component-wise minimum)
// ============================================================================

uvec3 test_uvec3_min_first_smaller() {
    // Function min() returns uvec3 (component-wise minimum)
    uvec3 a = uvec3(3u, 8u, 5u);
    uvec3 b = uvec3(7u, 4u, 9u);
    return min(a, b);
}

// run: test_uvec3_min_first_smaller() == uvec3(3u, 4u, 5u)

uvec3 test_uvec3_min_second_smaller() {
    uvec3 a = uvec3(7u, 8u, 9u);
    uvec3 b = uvec3(3u, 4u, 5u);
    return min(a, b);
}

// run: test_uvec3_min_second_smaller() == uvec3(3u, 4u, 5u)

uvec3 test_uvec3_min_mixed() {
    uvec3 a = uvec3(3u, 8u, 2u);
    uvec3 b = uvec3(7u, 4u, 9u);
    return min(a, b);
}

// run: test_uvec3_min_mixed() == uvec3(3u, 4u, 2u)

uvec3 test_uvec3_min_equal() {
    uvec3 a = uvec3(5u, 5u, 5u);
    uvec3 b = uvec3(5u, 5u, 5u);
    return min(a, b);
}

// run: test_uvec3_min_equal() == uvec3(5u, 5u, 5u)

uvec3 test_uvec3_min_zero() {
    uvec3 a = uvec3(0u, 5u, 1u);
    uvec3 b = uvec3(2u, 3u, 0u);
    return min(a, b);
}

// run: test_uvec3_min_zero() == uvec3(0u, 3u, 0u)

uvec3 test_uvec3_min_variables() {
    uvec3 a = uvec3(10u, 15u, 8u);
    uvec3 b = uvec3(12u, 10u, 12u);
    return min(a, b);
}

// run: test_uvec3_min_variables() == uvec3(10u, 10u, 8u)

uvec3 test_uvec3_min_expressions() {
    return min(uvec3(6u, 2u, 8u), uvec3(4u, 7u, 3u));
}

// run: test_uvec3_min_expressions() == uvec3(4u, 2u, 3u)

uvec3 test_uvec3_min_in_expression() {
    uvec3 a = uvec3(3u, 8u, 5u);
    uvec3 b = uvec3(7u, 4u, 9u);
    uvec3 c = uvec3(1u, 6u, 2u);
    return min(a, min(b, c));
}

// run: test_uvec3_min_in_expression() == uvec3(1u, 4u, 2u)
