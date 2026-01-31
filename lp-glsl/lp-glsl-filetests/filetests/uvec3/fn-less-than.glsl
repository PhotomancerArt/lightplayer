// test run
// target riscv32.q32

// ============================================================================
// Less Than: lessThan(uvec3, uvec3) -> bvec3 (component-wise)
// ============================================================================

bvec3 test_uvec3_less_than_mixed() {
    // Function lessThan() returns bvec3 (component-wise comparison)
    uvec3 a = uvec3(5u, 8u, 3u);
    uvec3 b = uvec3(7u, 6u, 4u);
    return lessThan(a, b);
}

// run: test_uvec3_less_than_mixed() == bvec3(true, false, true)

bvec3 test_uvec3_less_than_all_true() {
    uvec3 a = uvec3(1u, 2u, 3u);
    uvec3 b = uvec3(5u, 6u, 7u);
    return lessThan(a, b);
}

// run: test_uvec3_less_than_all_true() == bvec3(true, true, true)

bvec3 test_uvec3_less_than_all_false() {
    uvec3 a = uvec3(5u, 6u, 7u);
    uvec3 b = uvec3(1u, 2u, 3u);
    return lessThan(a, b);
}

// run: test_uvec3_less_than_all_false() == bvec3(false, false, false)

bvec3 test_uvec3_less_than_equal() {
    uvec3 a = uvec3(5u, 5u, 5u);
    uvec3 b = uvec3(5u, 6u, 4u);
    return lessThan(a, b);
}

// run: test_uvec3_less_than_equal() == bvec3(false, true, false)

bvec3 test_uvec3_less_than_zero() {
    uvec3 a = uvec3(0u, 1u, 2u);
    uvec3 b = uvec3(1u, 0u, 3u);
    return lessThan(a, b);
}

// run: test_uvec3_less_than_zero() == bvec3(true, false, true)

bvec3 test_uvec3_less_than_variables() {
    uvec3 a = uvec3(10u, 15u, 8u);
    uvec3 b = uvec3(12u, 10u, 12u);
    return lessThan(a, b);
}

// run: test_uvec3_less_than_variables() == bvec3(true, false, true)

bvec3 test_uvec3_less_than_expressions() {
    return lessThan(uvec3(3u, 7u, 2u), uvec3(5u, 5u, 4u));
}

// run: test_uvec3_less_than_expressions() == bvec3(true, false, true)

bool test_uvec3_less_than_in_expression() {
    uvec3 a = uvec3(1u, 5u, 3u);
    uvec3 b = uvec3(2u, 3u, 4u);
    uvec3 c = uvec3(3u, 7u, 1u);
    return lessThan(a, b) == lessThan(b, c);
    // (true,false,true) == (true,true,false) = false
}

// run: test_uvec3_less_than_in_expression() == false
