// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec2/fn-max --write
//
// test run
// target riscv32.q32

// ============================================================================
// Max: max(uvec2, uvec2) -> uvec2 (component-wise maximum)
// ============================================================================

uvec2 test_uvec2_max_first_larger() {
    // Function max() returns uvec2 (component-wise maximum)
    uvec2 a = uvec2(7u, 8u);
    uvec2 b = uvec2(3u, 4u);
    return max(a, b);
}

// run: test_uvec2_max_first_larger() == uvec2(7u, 8u)

uvec2 test_uvec2_max_second_larger() {
    uvec2 a = uvec2(3u, 4u);
    uvec2 b = uvec2(7u, 8u);
    return max(a, b);
}

// run: test_uvec2_max_second_larger() == uvec2(7u, 8u)

uvec2 test_uvec2_max_mixed() {
    uvec2 a = uvec2(3u, 8u);
    uvec2 b = uvec2(7u, 4u);
    return max(a, b);
}

// run: test_uvec2_max_mixed() == uvec2(7u, 8u)

uvec2 test_uvec2_max_equal() {
    uvec2 a = uvec2(5u, 5u);
    uvec2 b = uvec2(5u, 5u);
    return max(a, b);
}

// run: test_uvec2_max_equal() == uvec2(5u, 5u)

uvec2 test_uvec2_max_zero() {
    uvec2 a = uvec2(0u, 5u);
    uvec2 b = uvec2(2u, 3u);
    return max(a, b);
}

// run: test_uvec2_max_zero() == uvec2(2u, 5u)

uvec2 test_uvec2_max_variables() {
    uvec2 a = uvec2(10u, 15u);
    uvec2 b = uvec2(12u, 10u);
    return max(a, b);
}

// run: test_uvec2_max_variables() == uvec2(12u, 15u)

uvec2 test_uvec2_max_expressions() {
    return max(uvec2(6u, 2u), uvec2(4u, 7u));
}

// run: test_uvec2_max_expressions() == uvec2(6u, 7u)

uvec2 test_uvec2_max_in_expression() {
    uvec2 a = uvec2(3u, 8u);
    uvec2 b = uvec2(7u, 4u);
    uvec2 c = uvec2(1u, 6u);
    return max(a, max(b, c));
}

// run: test_uvec2_max_in_expression() == uvec2(7u, 8u)
