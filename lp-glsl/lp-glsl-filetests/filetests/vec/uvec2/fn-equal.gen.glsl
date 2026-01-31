// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec2/fn-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: equal(uvec2, uvec2) -> bvec2 (component-wise)
// ============================================================================

bvec2 test_uvec2_equal_function_mixed() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(5u, 4u);
    // Function equal() returns bvec2 (component-wise equality)
    return equal(a, b);
}

// run: test_uvec2_equal_function_mixed() == bvec2(true, false)

bvec2 test_uvec2_equal_function_all_true() {
    uvec2 a = uvec2(10u, 20u);
    uvec2 b = uvec2(10u, 20u);
    return equal(a, b);
}

// run: test_uvec2_equal_function_all_true() == bvec2(true, true)

bvec2 test_uvec2_equal_function_all_false() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(2u, 4u);
    return equal(a, b);
}

// run: test_uvec2_equal_function_all_false() == bvec2(false, false)

bvec2 test_uvec2_equal_function_zero() {
    uvec2 a = uvec2(0u, 5u);
    uvec2 b = uvec2(0u, 3u);
    return equal(a, b);
}

// run: test_uvec2_equal_function_zero() == bvec2(true, false)

bvec2 test_uvec2_equal_function_max_values() {
    uvec2 a = uvec2(4294967295u, 4294967294u);
    uvec2 b = uvec2(4294967295u, 4294967294u);
    return equal(a, b);
}

// run: test_uvec2_equal_function_max_values() == bvec2(true, true)

bvec2 test_uvec2_equal_function_variables() {
    uvec2 a = uvec2(8u, 12u);
    uvec2 b = uvec2(8u, 10u);
    return equal(a, b);
}

// run: test_uvec2_equal_function_variables() == bvec2(true, false)

bvec2 test_uvec2_equal_function_expressions() {
    return equal(uvec2(2u, 5u), uvec2(2u, 4u));
}

// run: test_uvec2_equal_function_expressions() == bvec2(true, false)

bool test_uvec2_equal_function_in_expression() {
    uvec2 a = uvec2(1u, 3u);
    uvec2 b = uvec2(1u, 4u);
    uvec2 c = uvec2(2u, 3u);
    return equal(a, b) == equal(b, c);
    // (true,false) == (false,false) = false
}

// run: test_uvec2_equal_function_in_expression() == false
