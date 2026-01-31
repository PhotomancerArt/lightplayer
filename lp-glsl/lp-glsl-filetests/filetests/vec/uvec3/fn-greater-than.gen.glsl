// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec3/fn-greater-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than: greaterThan(uvec3, uvec3) -> bvec3 (component-wise)
// ============================================================================

bvec3 test_uvec3_greater_than_mixed() {
    // Function greaterThan() returns bvec3 (component-wise comparison)
    uvec3 a = uvec3(7u, 6u, 9u);
    uvec3 b = uvec3(5u, 8u, 7u);
    return greaterThan(a, b);
}

// run: test_uvec3_greater_than_mixed() == bvec3(true, false, true)

bvec3 test_uvec3_greater_than_all_true() {
    uvec3 a = uvec3(5u, 6u, 7u);
    uvec3 b = uvec3(1u, 2u, 3u);
    return greaterThan(a, b);
}

// run: test_uvec3_greater_than_all_true() == bvec3(true, true, true)

bvec3 test_uvec3_greater_than_all_false() {
    uvec3 a = uvec3(1u, 2u, 3u);
    uvec3 b = uvec3(5u, 6u, 7u);
    return greaterThan(a, b);
}

// run: test_uvec3_greater_than_all_false() == bvec3(false, false, false)

bvec3 test_uvec3_greater_than_equal() {
    uvec3 a = uvec3(5u, 6u, 7u);
    uvec3 b = uvec3(5u, 5u, 8u);
    return greaterThan(a, b);
}

// run: test_uvec3_greater_than_equal() == bvec3(false, true, false)

bvec3 test_uvec3_greater_than_zero() {
    uvec3 a = uvec3(1u, 0u, 3u);
    uvec3 b = uvec3(0u, 1u, 2u);
    return greaterThan(a, b);
}

// run: test_uvec3_greater_than_zero() == bvec3(true, false, true)

bvec3 test_uvec3_greater_than_variables() {
    uvec3 a = uvec3(12u, 10u, 8u);
    uvec3 b = uvec3(10u, 15u, 9u);
    return greaterThan(a, b);
}

// run: test_uvec3_greater_than_variables() == bvec3(true, false, false)

bvec3 test_uvec3_greater_than_expressions() {
    return greaterThan(uvec3(5u, 5u, 6u), uvec3(3u, 7u, 8u));
}

// run: test_uvec3_greater_than_expressions() == bvec3(true, false, false)

bvec3 test_uvec3_greater_than_in_expression() {
    uvec3 a = uvec3(3u, 7u, 5u);
    uvec3 b = uvec3(2u, 3u, 6u);
    uvec3 c = uvec3(1u, 5u, 4u);
    // Use equal() for component-wise comparison of bvec3 values
    // greaterThan(a, b) = (true,true,false)
    // greaterThan(b, c) = (true,false,true)
    // equal(greaterThan(a, b), greaterThan(b, c)) = (true,false,false)
    return equal(greaterThan(a, b), greaterThan(b, c));
}

// run: test_uvec3_greater_than_in_expression() == bvec3(true, false, false)
