// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec2/fn-greater-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than Equal: greaterThanEqual(uvec2, uvec2) -> bvec2 (component-wise)
// ============================================================================

bvec2 test_uvec2_greater_equal_mixed() {
    // Function greaterThanEqual() returns bvec2 (component-wise comparison)
    uvec2 a = uvec2(7u, 6u);
    uvec2 b = uvec2(5u, 8u);
    return greaterThanEqual(a, b);
}

// run: test_uvec2_greater_equal_mixed() == bvec2(true, false)

bvec2 test_uvec2_greater_equal_all_true() {
    uvec2 a = uvec2(5u, 6u);
    uvec2 b = uvec2(1u, 2u);
    return greaterThanEqual(a, b);
}

// run: test_uvec2_greater_equal_all_true() == bvec2(true, true)

bvec2 test_uvec2_greater_equal_all_false() {
    uvec2 a = uvec2(1u, 2u);
    uvec2 b = uvec2(5u, 6u);
    return greaterThanEqual(a, b);
}

// run: test_uvec2_greater_equal_all_false() == bvec2(false, false)

bvec2 test_uvec2_greater_equal_equal() {
    uvec2 a = uvec2(5u, 5u);
    uvec2 b = uvec2(5u, 5u);
    return greaterThanEqual(a, b);
}

// run: test_uvec2_greater_equal_equal() == bvec2(true, true)

bvec2 test_uvec2_greater_equal_mixed_equal() {
    uvec2 a = uvec2(5u, 6u);
    uvec2 b = uvec2(5u, 5u);
    return greaterThanEqual(a, b);
}

// run: test_uvec2_greater_equal_mixed_equal() == bvec2(true, true)

bvec2 test_uvec2_greater_equal_zero() {
    uvec2 a = uvec2(1u, 0u);
    uvec2 b = uvec2(0u, 1u);
    return greaterThanEqual(a, b);
}

// run: test_uvec2_greater_equal_zero() == bvec2(true, false)

bvec2 test_uvec2_greater_equal_variables() {
    uvec2 a = uvec2(12u, 10u);
    uvec2 b = uvec2(10u, 15u);
    return greaterThanEqual(a, b);
}

// run: test_uvec2_greater_equal_variables() == bvec2(true, false)

bvec2 test_uvec2_greater_equal_expressions() {
    return greaterThanEqual(uvec2(5u, 5u), uvec2(3u, 7u));
}

// run: test_uvec2_greater_equal_expressions() == bvec2(true, false)

bvec2 test_uvec2_greater_equal_in_expression() {
    uvec2 a = uvec2(3u, 7u);
    uvec2 b = uvec2(2u, 3u);
    uvec2 c = uvec2(1u, 5u);
    // Use equal() for component-wise comparison of bvec2 values
    // greaterThanEqual(a, b) = (true,true)
    // greaterThanEqual(b, c) = (true,false)
    // equal(greaterThanEqual(a, b), greaterThanEqual(b, c)) = (true,false)
    return equal(greaterThanEqual(a, b), greaterThanEqual(b, c));
}

// run: test_uvec2_greater_equal_in_expression() == bvec2(true, false)
