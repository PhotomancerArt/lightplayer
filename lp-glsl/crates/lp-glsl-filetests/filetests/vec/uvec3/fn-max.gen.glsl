// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec3/fn-max --write
//
// test run
// target riscv32.q32

// ============================================================================
// Max: max(uvec3, uvec3) -> uvec3 (component-wise maximum)
// ============================================================================

uvec3 test_uvec3_max_first_larger() {
    // Function max() returns uvec3 (component-wise maximum)
    uvec3 a = uvec3(7u, 8u, 9u);
    uvec3 b = uvec3(3u, 4u, 5u);
    return max(a, b);
}

// run: test_uvec3_max_first_larger() == uvec3(7u, 8u, 9u)

uvec3 test_uvec3_max_second_larger() {
    uvec3 a = uvec3(3u, 4u, 5u);
    uvec3 b = uvec3(7u, 8u, 9u);
    return max(a, b);
}

// run: test_uvec3_max_second_larger() == uvec3(7u, 8u, 9u)

uvec3 test_uvec3_max_mixed() {
    uvec3 a = uvec3(3u, 8u, 2u);
    uvec3 b = uvec3(7u, 4u, 9u);
    return max(a, b);
}

// run: test_uvec3_max_mixed() == uvec3(7u, 8u, 9u)

uvec3 test_uvec3_max_equal() {
    uvec3 a = uvec3(5u, 5u, 5u);
    uvec3 b = uvec3(5u, 5u, 5u);
    return max(a, b);
}

// run: test_uvec3_max_equal() == uvec3(5u, 5u, 5u)

uvec3 test_uvec3_max_zero() {
    uvec3 a = uvec3(0u, 5u, 1u);
    uvec3 b = uvec3(2u, 3u, 0u);
    return max(a, b);
}

// run: test_uvec3_max_zero() == uvec3(2u, 5u, 1u)

uvec3 test_uvec3_max_variables() {
    uvec3 a = uvec3(10u, 15u, 8u);
    uvec3 b = uvec3(12u, 10u, 12u);
    return max(a, b);
}

// run: test_uvec3_max_variables() == uvec3(12u, 15u, 12u)

uvec3 test_uvec3_max_expressions() {
    return max(uvec3(6u, 2u, 8u), uvec3(4u, 7u, 3u));
}

// run: test_uvec3_max_expressions() == uvec3(6u, 7u, 8u)

uvec3 test_uvec3_max_in_expression() {
    uvec3 a = uvec3(3u, 8u, 5u);
    uvec3 b = uvec3(7u, 4u, 9u);
    uvec3 c = uvec3(1u, 6u, 2u);
    return max(a, max(b, c));
}

// run: test_uvec3_max_in_expression() == uvec3(7u, 8u, 9u)
