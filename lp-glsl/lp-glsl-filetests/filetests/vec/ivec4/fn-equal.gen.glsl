// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/fn-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: equal(ivec4, ivec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_ivec4_equal_function_mixed() {
    ivec4 a = ivec4(5, 3, 7, 2);
    ivec4 b = ivec4(5, 4, 7, 2);
    // Function equal() returns bvec4 (component-wise equality)
    return equal(a, b);
}

// run: test_ivec4_equal_function_mixed() == bvec4(true, false, true, true)

bvec4 test_ivec4_equal_function_all_true() {
    ivec4 a = ivec4(10, 20, 30, 40);
    ivec4 b = ivec4(10, 20, 30, 40);
    return equal(a, b);
}

// run: test_ivec4_equal_function_all_true() == bvec4(true, true, true, true)

bvec4 test_ivec4_equal_function_all_false() {
    ivec4 a = ivec4(5, 3, 7, 2);
    ivec4 b = ivec4(2, 4, 1, 3);
    return equal(a, b);
}

// run: test_ivec4_equal_function_all_false() == bvec4(false, false, false, false)

bvec4 test_ivec4_equal_function_zero() {
    ivec4 a = ivec4(0, 5, 0, 2);
    ivec4 b = ivec4(0, 3, 1, 2);
    return equal(a, b);
}

// run: test_ivec4_equal_function_zero() == bvec4(true, false, false, true)

bvec4 test_ivec4_equal_function_negative() {
    ivec4 a = ivec4(-5, -3, -7, -2);
    ivec4 b = ivec4(-5, -4, -7, -1);
    return equal(a, b);
}

// run: test_ivec4_equal_function_negative() == bvec4(true, false, true, false)

bvec4 test_ivec4_equal_function_variables() {
    ivec4 a = ivec4(8, 12, 6, 9);
    ivec4 b = ivec4(8, 10, 7, 9);
    return equal(a, b);
}

// run: test_ivec4_equal_function_variables() == bvec4(true, false, false, true)

bvec4 test_ivec4_equal_function_expressions() {
    return equal(ivec4(2, 5, 3, 8), ivec4(2, 4, 8, 8));
}

// run: test_ivec4_equal_function_expressions() == bvec4(true, false, false, true)

bvec4 test_ivec4_equal_function_in_expression() {
    ivec4 a = ivec4(1, 3, 5, 7);
    ivec4 b = ivec4(1, 4, 5, 7);
    ivec4 c = ivec4(2, 3, 5, 6);
    // Use equal() for component-wise comparison of bvec4 values
    // equal(a, b) = (true,false,true,true)
    // equal(b, c) = (false,false,true,false)
    // equal(equal(a, b), equal(b, c)) = (false,true,true,false)
    return equal(equal(a, b), equal(b, c));
}

// run: test_ivec4_equal_function_in_expression() == bvec4(false, true, true, false)
