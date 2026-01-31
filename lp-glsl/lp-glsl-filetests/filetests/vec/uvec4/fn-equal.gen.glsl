// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec4/fn-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: equal(uvec4, uvec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_uvec4_equal_function_mixed() {
    uvec4 a = uvec4(5u, 3u, 7u, 2u);
    uvec4 b = uvec4(5u, 4u, 7u, 2u);
    // Function equal() returns bvec4 (component-wise equality)
    return equal(a, b);
}

// run: test_uvec4_equal_function_mixed() == bvec4(true, false, true, true)

bvec4 test_uvec4_equal_function_all_true() {
    uvec4 a = uvec4(10u, 20u, 30u, 40u);
    uvec4 b = uvec4(10u, 20u, 30u, 40u);
    return equal(a, b);
}

// run: test_uvec4_equal_function_all_true() == bvec4(true, true, true, true)

bvec4 test_uvec4_equal_function_all_false() {
    uvec4 a = uvec4(5u, 3u, 7u, 2u);
    uvec4 b = uvec4(2u, 4u, 1u, 3u);
    return equal(a, b);
}

// run: test_uvec4_equal_function_all_false() == bvec4(false, false, false, false)

bvec4 test_uvec4_equal_function_zero() {
    uvec4 a = uvec4(0u, 5u, 0u, 2u);
    uvec4 b = uvec4(0u, 3u, 1u, 2u);
    return equal(a, b);
}

// run: test_uvec4_equal_function_zero() == bvec4(true, false, false, true)

bvec4 test_uvec4_equal_function_max_values() {
    uvec4 a = uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u);
    uvec4 b = uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u);
    return equal(a, b);
}

// run: test_uvec4_equal_function_max_values() == bvec4(true, true, true, true)

bvec4 test_uvec4_equal_function_variables() {
    uvec4 a = uvec4(8u, 12u, 6u, 9u);
    uvec4 b = uvec4(8u, 10u, 7u, 9u);
    return equal(a, b);
}

// run: test_uvec4_equal_function_variables() == bvec4(true, false, false, true)

bvec4 test_uvec4_equal_function_expressions() {
    return equal(uvec4(2u, 5u, 3u, 8u), uvec4(2u, 4u, 8u, 8u));
}

// run: test_uvec4_equal_function_expressions() == bvec4(true, false, false, true)

bvec4 test_uvec4_equal_function_in_expression() {
    uvec4 a = uvec4(1u, 3u, 5u, 7u);
    uvec4 b = uvec4(1u, 4u, 5u, 7u);
    uvec4 c = uvec4(2u, 3u, 5u, 6u);
    // Use equal() for component-wise comparison of bvec4 values
    // equal(a, b) = (true,false,true,true)
    // equal(b, c) = (false,false,true,false)
    // equal(equal(a, b), equal(b, c)) = (false,true,true,false)
    return equal(equal(a, b), equal(b, c));
}

// run: test_uvec4_equal_function_in_expression() == bvec4(false, true, true, false)
