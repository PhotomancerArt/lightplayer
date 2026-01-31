// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec3/fn-greater-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Less Than Equal: lessThanEqual(uvec3, uvec3) -> bvec3 (component-wise)
// ============================================================================

bvec3 test_uvec3_less_equal_mixed() {
    // Function lessThanEqual() returns bvec3 (component-wise comparison)
    uvec3 a = uvec3(5u, 8u, 7u);
    uvec3 b = uvec3(7u, 6u, 9u);
    return lessThanEqual(a, b);
}

// run: test_uvec3_less_equal_mixed() == bvec3(true, false, true)

bvec3 test_uvec3_less_equal_all_true() {
    uvec3 a = uvec3(1u, 2u, 3u);
    uvec3 b = uvec3(5u, 6u, 7u);
    return lessThanEqual(a, b);
}

// run: test_uvec3_less_equal_all_true() == bvec3(true, true, true)

bvec3 test_uvec3_less_equal_all_false() {
    uvec3 a = uvec3(5u, 6u, 7u);
    uvec3 b = uvec3(1u, 2u, 3u);
    return lessThanEqual(a, b);
}

// run: test_uvec3_less_equal_all_false() == bvec3(false, false, false)

bvec3 test_uvec3_less_equal_equal() {
    uvec3 a = uvec3(5u, 5u, 5u);
    uvec3 b = uvec3(5u, 5u, 5u);
    return lessThanEqual(a, b);
}

// run: test_uvec3_less_equal_equal() == bvec3(true, true, true)

bvec3 test_uvec3_less_equal_mixed_equal() {
    uvec3 a = uvec3(5u, 6u, 7u);
    uvec3 b = uvec3(5u, 5u, 8u);
    return lessThanEqual(a, b);
}

// run: test_uvec3_less_equal_mixed_equal() == bvec3(true, false, true)

bvec3 test_uvec3_less_equal_zero() {
    uvec3 a = uvec3(0u, 1u, 2u);
    uvec3 b = uvec3(1u, 0u, 3u);
    return lessThanEqual(a, b);
}

// run: test_uvec3_less_equal_zero() == bvec3(true, false, true)

bvec3 test_uvec3_less_equal_variables() {
    uvec3 a = uvec3(10u, 15u, 8u);
    uvec3 b = uvec3(12u, 10u, 12u);
    return lessThanEqual(a, b);
}

// run: test_uvec3_less_equal_variables() == bvec3(true, false, true)

bvec3 test_uvec3_less_equal_expressions() {
    return lessThanEqual(uvec3(3u, 7u, 2u), uvec3(5u, 5u, 4u));
}

// run: test_uvec3_less_equal_expressions() == bvec3(true, false, true)

bvec3 test_uvec3_less_equal_in_expression() {
    uvec3 a = uvec3(1u, 5u, 4u);
    uvec3 b = uvec3(2u, 3u, 6u);
    uvec3 c = uvec3(3u, 7u, 5u);
    // Use equal() for component-wise comparison of bvec3 values
    // lessThanEqual(a, b) = (true,false,true)
    // lessThanEqual(b, c) = (true,true,false)
    // equal(lessThanEqual(a, b), lessThanEqual(b, c)) = (true,false,false)
    return equal(lessThanEqual(a, b), lessThanEqual(b, c));
}

// run: test_uvec3_less_equal_in_expression() == bvec3(true, false, false)
