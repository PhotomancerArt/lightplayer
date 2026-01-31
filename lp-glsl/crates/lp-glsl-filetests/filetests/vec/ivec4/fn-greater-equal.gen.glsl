// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/fn-greater-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than Equal: greaterThanEqual(ivec4, ivec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_ivec4_greater_equal_mixed() {
    // Function greaterThanEqual() returns bvec4 (component-wise comparison)
    ivec4 a = ivec4(7, 6, 9, 2);
    ivec4 b = ivec4(5, 8, 7, 4);
    return greaterThanEqual(a, b);
}

// run: test_ivec4_greater_equal_mixed() == bvec4(true, false, true, false)

bvec4 test_ivec4_greater_equal_all_true() {
    ivec4 a = ivec4(5, 6, 7, 8);
    ivec4 b = ivec4(1, 2, 3, 4);
    return greaterThanEqual(a, b);
}

// run: test_ivec4_greater_equal_all_true() == bvec4(true, true, true, true)

bvec4 test_ivec4_greater_equal_all_false() {
    ivec4 a = ivec4(1, 2, 3, 4);
    ivec4 b = ivec4(5, 6, 7, 8);
    return greaterThanEqual(a, b);
}

// run: test_ivec4_greater_equal_all_false() == bvec4(false, false, false, false)

bvec4 test_ivec4_greater_equal_equal() {
    ivec4 a = ivec4(5, 5, 5, 5);
    ivec4 b = ivec4(5, 5, 5, 5);
    return greaterThanEqual(a, b);
}

// run: test_ivec4_greater_equal_equal() == bvec4(true, true, true, true)

bvec4 test_ivec4_greater_equal_mixed_equal() {
    ivec4 a = ivec4(5, 6, 7, 8);
    ivec4 b = ivec4(5, 5, 8, 7);
    return greaterThanEqual(a, b);
}

// run: test_ivec4_greater_equal_mixed_equal() == bvec4(true, true, false, true)

bvec4 test_ivec4_greater_equal_negative() {
    ivec4 a = ivec4(-1, -3, 2, -5);
    ivec4 b = ivec4(-5, -2, 1, -7);
    return greaterThanEqual(a, b);
}

// run: test_ivec4_greater_equal_negative() == bvec4(true, false, true, true)

bvec4 test_ivec4_greater_equal_zero() {
    ivec4 a = ivec4(1, 0, 3, -1);
    ivec4 b = ivec4(0, 1, 2, 0);
    return greaterThanEqual(a, b);
}

// run: test_ivec4_greater_equal_zero() == bvec4(true, false, true, false)

bvec4 test_ivec4_greater_equal_variables() {
    ivec4 a = ivec4(12, 10, 8, 6);
    ivec4 b = ivec4(10, 15, 8, 5);
    return greaterThanEqual(a, b);
}

// run: test_ivec4_greater_equal_variables() == bvec4(true, false, true, true)

bvec4 test_ivec4_greater_equal_expressions() {
    return greaterThanEqual(ivec4(5, 5, 6, 3), ivec4(3, 7, 6, 4));
}

// run: test_ivec4_greater_equal_expressions() == bvec4(true, false, true, false)

bvec4 test_ivec4_greater_equal_in_expression() {
    ivec4 a = ivec4(3, 7, 5, 9);
    ivec4 b = ivec4(2, 3, 6, 8);
    ivec4 c = ivec4(1, 5, 4, 7);
    // Use equal() for component-wise comparison of bvec4 values
    // greaterThanEqual(a, b) = (true,true,false,true)
    // greaterThanEqual(b, c) = (true,false,true,true)
    // equal(greaterThanEqual(a, b), greaterThanEqual(b, c)) = (true,false,false,true)
    return equal(greaterThanEqual(a, b), greaterThanEqual(b, c));
}

// run: test_ivec4_greater_equal_in_expression() == bvec4(true, false, false, true)
