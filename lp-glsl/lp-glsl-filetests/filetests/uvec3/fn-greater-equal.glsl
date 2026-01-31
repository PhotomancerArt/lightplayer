// test run
// target riscv32.q32

// ============================================================================
// Greater Than Equal: greaterThanEqual(uvec3, uvec3) -> bvec3 (component-wise)
// ============================================================================

bvec3 test_uvec3_greater_equal_mixed() {
    // Function greaterThanEqual() returns bvec3 (component-wise comparison)
    uvec3 a = uvec3(7u, 6u, 4u);
    uvec3 b = uvec3(5u, 8u, 3u);
    return greaterThanEqual(a, b);
}

// run: test_uvec3_greater_equal_mixed() == bvec3(true, false, true)

bvec3 test_uvec3_greater_equal_all_true() {
    uvec3 a = uvec3(5u, 6u, 7u);
    uvec3 b = uvec3(1u, 2u, 3u);
    return greaterThanEqual(a, b);
}

// run: test_uvec3_greater_equal_all_true() == bvec3(true, true, true)

bvec3 test_uvec3_greater_equal_all_false() {
    uvec3 a = uvec3(1u, 2u, 3u);
    uvec3 b = uvec3(5u, 6u, 7u);
    return greaterThanEqual(a, b);
}

// run: test_uvec3_greater_equal_all_false() == bvec3(false, false, false)

bvec3 test_uvec3_greater_equal_equal() {
    uvec3 a = uvec3(5u, 5u, 5u);
    uvec3 b = uvec3(5u, 5u, 5u);
    return greaterThanEqual(a, b);
}

// run: test_uvec3_greater_equal_equal() == bvec3(true, true, true)

bvec3 test_uvec3_greater_equal_mixed_equal() {
    uvec3 a = uvec3(5u, 6u, 4u);
    uvec3 b = uvec3(5u, 5u, 5u);
    return greaterThanEqual(a, b);
}

// run: test_uvec3_greater_equal_mixed_equal() == bvec3(true, true, false)

bvec3 test_uvec3_greater_equal_zero() {
    uvec3 a = uvec3(1u, 0u, 3u);
    uvec3 b = uvec3(0u, 1u, 2u);
    return greaterThanEqual(a, b);
}

// run: test_uvec3_greater_equal_zero() == bvec3(true, false, true)

bvec3 test_uvec3_greater_equal_variables() {
    uvec3 a = uvec3(12u, 10u, 12u);
    uvec3 b = uvec3(10u, 15u, 8u);
    return greaterThanEqual(a, b);
}

// run: test_uvec3_greater_equal_variables() == bvec3(true, false, true)

bvec3 test_uvec3_greater_equal_expressions() {
    return greaterThanEqual(uvec3(5u, 5u, 4u), uvec3(3u, 7u, 2u));
}

// run: test_uvec3_greater_equal_expressions() == bvec3(true, false, true)

bool test_uvec3_greater_equal_in_expression() {
    uvec3 a = uvec3(3u, 7u, 1u);
    uvec3 b = uvec3(2u, 3u, 4u);
    uvec3 c = uvec3(1u, 5u, 3u);
    return greaterThanEqual(a, b) == greaterThanEqual(b, c);
    // (true,true,false) == (true,false,true) = false
}

// run: test_uvec3_greater_equal_in_expression() == false
