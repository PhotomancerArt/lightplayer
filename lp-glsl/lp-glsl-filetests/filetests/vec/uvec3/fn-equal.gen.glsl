// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec3/fn-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: equal(uvec3, uvec3) -> bvec3 (component-wise)
// ============================================================================

bvec3 test_uvec3_equal_function_mixed() {
    uvec3 a = uvec3(5u, 3u, 7u);
    uvec3 b = uvec3(5u, 4u, 7u);
    // Function equal() returns bvec3 (component-wise equality)
    return equal(a, b);
}

// run: test_uvec3_equal_function_mixed() == bvec3(true, false, true)

bvec3 test_uvec3_equal_function_all_true() {
    uvec3 a = uvec3(10u, 20u, 30u);
    uvec3 b = uvec3(10u, 20u, 30u);
    return equal(a, b);
}

// run: test_uvec3_equal_function_all_true() == bvec3(true, true, true)

bvec3 test_uvec3_equal_function_all_false() {
    uvec3 a = uvec3(5u, 3u, 7u);
    uvec3 b = uvec3(2u, 4u, 1u);
    return equal(a, b);
}

// run: test_uvec3_equal_function_all_false() == bvec3(false, false, false)

bvec3 test_uvec3_equal_function_zero() {
    uvec3 a = uvec3(0u, 5u, 0u);
    uvec3 b = uvec3(0u, 3u, 1u);
    return equal(a, b);
}

// run: test_uvec3_equal_function_zero() == bvec3(true, false, false)

bvec3 test_uvec3_equal_function_max_values() {
    uvec3 a = uvec3(4294967295u, 4294967294u, 4294967293u);
    uvec3 b = uvec3(4294967295u, 4294967294u, 4294967293u);
    return equal(a, b);
}

// run: test_uvec3_equal_function_max_values() == bvec3(true, true, true)

bvec3 test_uvec3_equal_function_variables() {
    uvec3 a = uvec3(8u, 12u, 6u);
    uvec3 b = uvec3(8u, 10u, 7u);
    return equal(a, b);
}

// run: test_uvec3_equal_function_variables() == bvec3(true, false, false)

bvec3 test_uvec3_equal_function_expressions() {
    return equal(uvec3(2u, 5u, 3u), uvec3(2u, 4u, 8u));
}

// run: test_uvec3_equal_function_expressions() == bvec3(true, false, false)

bvec3 test_uvec3_equal_function_in_expression() {
    uvec3 a = uvec3(1u, 3u, 5u);
    uvec3 b = uvec3(1u, 4u, 5u);
    uvec3 c = uvec3(2u, 3u, 5u);
    // Use equal() for component-wise comparison of bvec3 values
    // equal(a, b) = (true,false,true)
    // equal(b, c) = (false,false,true)
    // equal(equal(a, b), equal(b, c)) = (false,true,true)
    return equal(equal(a, b), equal(b, c));
}

// run: test_uvec3_equal_function_in_expression() == bvec3(false, true, true)
