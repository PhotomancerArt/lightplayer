// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec4/fn-less-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Less Than: lessThan(uvec4, uvec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_uvec4_less_than_mixed() {
    // Function lessThan() returns bvec4 (component-wise comparison)
    uvec4 a = uvec4(5u, 8u, 7u, 4u);
    uvec4 b = uvec4(7u, 6u, 9u, 2u);
    return lessThan(a, b);
}

// run: test_uvec4_less_than_mixed() == bvec4(true, false, true, false)

bvec4 test_uvec4_less_than_all_true() {
    uvec4 a = uvec4(1u, 2u, 3u, 4u);
    uvec4 b = uvec4(5u, 6u, 7u, 8u);
    return lessThan(a, b);
}

// run: test_uvec4_less_than_all_true() == bvec4(true, true, true, true)

bvec4 test_uvec4_less_than_all_false() {
    uvec4 a = uvec4(5u, 6u, 7u, 8u);
    uvec4 b = uvec4(1u, 2u, 3u, 4u);
    return lessThan(a, b);
}

// run: test_uvec4_less_than_all_false() == bvec4(false, false, false, false)

bvec4 test_uvec4_less_than_equal() {
    uvec4 a = uvec4(5u, 5u, 5u, 5u);
    uvec4 b = uvec4(5u, 6u, 4u, 7u);
    return lessThan(a, b);
}

// run: test_uvec4_less_than_equal() == bvec4(false, true, false, true)

bvec4 test_uvec4_less_than_zero() {
    uvec4 a = uvec4(0u, 1u, 2u, 0u);
    uvec4 b = uvec4(1u, 0u, 3u, 2u);
    return lessThan(a, b);
}

// run: test_uvec4_less_than_zero() == bvec4(true, false, true, true)

bvec4 test_uvec4_less_than_variables() {
    uvec4 a = uvec4(10u, 15u, 8u, 12u);
    uvec4 b = uvec4(12u, 10u, 12u, 8u);
    return lessThan(a, b);
}

// run: test_uvec4_less_than_variables() == bvec4(true, false, true, false)

bvec4 test_uvec4_less_than_expressions() {
    return lessThan(uvec4(3u, 7u, 2u, 9u), uvec4(5u, 5u, 4u, 8u));
}

// run: test_uvec4_less_than_expressions() == bvec4(true, false, true, false)

bool test_uvec4_less_than_in_expression() {
    uvec4 a = uvec4(1u, 5u, 3u, 7u);
    uvec4 b = uvec4(2u, 3u, 4u, 5u);
    uvec4 c = uvec4(3u, 7u, 1u, 9u);
    return lessThan(a, b) == lessThan(b, c);
    // (true,false,true,false) == (true,true,false,true) = false
}

// run: test_uvec4_less_than_in_expression() == false
