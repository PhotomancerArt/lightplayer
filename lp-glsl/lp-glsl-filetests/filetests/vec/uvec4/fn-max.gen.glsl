// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec4/fn-max --write
//
// test run
// target riscv32.q32

// ============================================================================
// Max: max(uvec4, uvec4) -> uvec4 (component-wise maximum)
// ============================================================================

uvec4 test_uvec4_max_first_larger() {
    // Function max() returns uvec4 (component-wise maximum)
    uvec4 a = uvec4(7u, 8u, 9u, 6u);
    uvec4 b = uvec4(3u, 4u, 5u, 1u);
    return max(a, b);
}

// run: test_uvec4_max_first_larger() == uvec4(7u, 8u, 9u, 6u)

uvec4 test_uvec4_max_second_larger() {
    uvec4 a = uvec4(3u, 4u, 5u, 1u);
    uvec4 b = uvec4(7u, 8u, 9u, 6u);
    return max(a, b);
}

// run: test_uvec4_max_second_larger() == uvec4(7u, 8u, 9u, 6u)

uvec4 test_uvec4_max_mixed() {
    uvec4 a = uvec4(3u, 8u, 2u, 7u);
    uvec4 b = uvec4(7u, 4u, 9u, 3u);
    return max(a, b);
}

// run: test_uvec4_max_mixed() == uvec4(7u, 8u, 9u, 7u)

uvec4 test_uvec4_max_equal() {
    uvec4 a = uvec4(5u, 5u, 5u, 5u);
    uvec4 b = uvec4(5u, 5u, 5u, 5u);
    return max(a, b);
}

// run: test_uvec4_max_equal() == uvec4(5u, 5u, 5u, 5u)

uvec4 test_uvec4_max_zero() {
    uvec4 a = uvec4(0u, 5u, 1u, 2u);
    uvec4 b = uvec4(2u, 3u, 0u, 1u);
    return max(a, b);
}

// run: test_uvec4_max_zero() == uvec4(2u, 5u, 1u, 2u)

uvec4 test_uvec4_max_variables() {
    uvec4 a = uvec4(10u, 15u, 8u, 12u);
    uvec4 b = uvec4(12u, 10u, 12u, 8u);
    return max(a, b);
}

// run: test_uvec4_max_variables() == uvec4(12u, 15u, 12u, 12u)

uvec4 test_uvec4_max_expressions() {
    return max(uvec4(6u, 2u, 8u, 4u), uvec4(4u, 7u, 3u, 9u));
}

// run: test_uvec4_max_expressions() == uvec4(6u, 7u, 8u, 9u)

uvec4 test_uvec4_max_in_expression() {
    uvec4 a = uvec4(3u, 8u, 5u, 2u);
    uvec4 b = uvec4(7u, 4u, 9u, 7u);
    uvec4 c = uvec4(1u, 6u, 2u, 3u);
    return max(a, max(b, c));
}

// run: test_uvec4_max_in_expression() == uvec4(7u, 8u, 9u, 7u)
