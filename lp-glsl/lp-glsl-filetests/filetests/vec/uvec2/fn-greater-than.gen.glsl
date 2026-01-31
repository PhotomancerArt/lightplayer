// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec2/fn-greater-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than: greaterThan(uvec2, uvec2) -> bvec2 (component-wise)
// ============================================================================

bvec2 test_uvec2_greater_than_mixed() {
    // Function greaterThan() returns bvec2 (component-wise comparison)
    uvec2 a = uvec2(7u, 6u);
    uvec2 b = uvec2(5u, 8u);
    return greaterThan(a, b);
}

// run: test_uvec2_greater_than_mixed() == bvec2(true, false)

bvec2 test_uvec2_greater_than_all_true() {
    uvec2 a = uvec2(5u, 6u);
    uvec2 b = uvec2(1u, 2u);
    return greaterThan(a, b);
}

// run: test_uvec2_greater_than_all_true() == bvec2(true, true)

bvec2 test_uvec2_greater_than_all_false() {
    uvec2 a = uvec2(1u, 2u);
    uvec2 b = uvec2(5u, 6u);
    return greaterThan(a, b);
}

// run: test_uvec2_greater_than_all_false() == bvec2(false, false)

bvec2 test_uvec2_greater_than_equal() {
    uvec2 a = uvec2(5u, 6u);
    uvec2 b = uvec2(5u, 5u);
    return greaterThan(a, b);
}

// run: test_uvec2_greater_than_equal() == bvec2(false, true)

bvec2 test_uvec2_greater_than_zero() {
    uvec2 a = uvec2(1u, 0u);
    uvec2 b = uvec2(0u, 1u);
    return greaterThan(a, b);
}

// run: test_uvec2_greater_than_zero() == bvec2(true, false)

bvec2 test_uvec2_greater_than_variables() {
    uvec2 a = uvec2(12u, 10u);
    uvec2 b = uvec2(10u, 15u);
    return greaterThan(a, b);
}

// run: test_uvec2_greater_than_variables() == bvec2(true, false)

bvec2 test_uvec2_greater_than_expressions() {
    return greaterThan(uvec2(5u, 5u), uvec2(3u, 7u));
}

// run: test_uvec2_greater_than_expressions() == bvec2(true, false)

bvec2 test_uvec2_greater_than_in_expression() {
    uvec2 a = uvec2(3u, 7u);
    uvec2 b = uvec2(2u, 3u);
    uvec2 c = uvec2(1u, 5u);
    // Use equal() for component-wise comparison of bvec2 values
    // greaterThan(a, b) = (true,true)
    // greaterThan(b, c) = (true,false)
    // equal(greaterThan(a, b), greaterThan(b, c)) = (true,false)
    return equal(greaterThan(a, b), greaterThan(b, c));
}

// run: test_uvec2_greater_than_in_expression() == bvec2(true, false)
