// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/fn-greater-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Less Than Equal: lessThanEqual(ivec4, ivec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_ivec4_less_equal_mixed() {
    // Function lessThanEqual() returns bvec4 (component-wise comparison)
    ivec4 a = ivec4(5, 8, 7, 4);
    ivec4 b = ivec4(7, 6, 9, 2);
    return lessThanEqual(a, b);
}

// run: test_ivec4_less_equal_mixed() == bvec4(true, false, true, false)

bvec4 test_ivec4_less_equal_all_true() {
    ivec4 a = ivec4(1, 2, 3, 4);
    ivec4 b = ivec4(5, 6, 7, 8);
    return lessThanEqual(a, b);
}

// run: test_ivec4_less_equal_all_true() == bvec4(true, true, true, true)

bvec4 test_ivec4_less_equal_all_false() {
    ivec4 a = ivec4(5, 6, 7, 8);
    ivec4 b = ivec4(1, 2, 3, 4);
    return lessThanEqual(a, b);
}

// run: test_ivec4_less_equal_all_false() == bvec4(false, false, false, false)

bvec4 test_ivec4_less_equal_equal() {
    ivec4 a = ivec4(5, 5, 5, 5);
    ivec4 b = ivec4(5, 5, 5, 5);
    return lessThanEqual(a, b);
}

// run: test_ivec4_less_equal_equal() == bvec4(true, true, true, true)

bvec4 test_ivec4_less_equal_mixed_equal() {
    ivec4 a = ivec4(5, 6, 7, 8);
    ivec4 b = ivec4(5, 5, 8, 7);
    return lessThanEqual(a, b);
}

// run: test_ivec4_less_equal_mixed_equal() == bvec4(true, false, true, false)

bvec4 test_ivec4_less_equal_negative() {
    ivec4 a = ivec4(-5, -2, 1, -7);
    ivec4 b = ivec4(-1, -3, 2, -5);
    return lessThanEqual(a, b);
}

// run: test_ivec4_less_equal_negative() == bvec4(true, false, true, true)

bvec4 test_ivec4_less_equal_zero() {
    ivec4 a = ivec4(0, 1, 2, 0);
    ivec4 b = ivec4(1, 0, 3, -1);
    return lessThanEqual(a, b);
}

// run: test_ivec4_less_equal_zero() == bvec4(true, false, true, false)

bvec4 test_ivec4_less_equal_variables() {
    ivec4 a = ivec4(10, 15, 8, 12);
    ivec4 b = ivec4(12, 10, 12, 8);
    return lessThanEqual(a, b);
}

// run: test_ivec4_less_equal_variables() == bvec4(true, false, true, false)

bvec4 test_ivec4_less_equal_expressions() {
    return lessThanEqual(ivec4(3, 7, 2, 9), ivec4(5, 5, 4, 8));
}

// run: test_ivec4_less_equal_expressions() == bvec4(true, false, true, false)

bvec4 test_ivec4_less_equal_in_expression() {
    ivec4 a = ivec4(1, 5, 4, 7);
    ivec4 b = ivec4(2, 3, 6, 8);
    ivec4 c = ivec4(3, 7, 5, 9);
    // Use equal() for component-wise comparison of bvec4 values
    // lessThanEqual(a, b) = (true,false,true,true)
    // lessThanEqual(b, c) = (true,true,false,true)
    // equal(lessThanEqual(a, b), lessThanEqual(b, c)) = (true,false,false,true)
    return equal(lessThanEqual(a, b), lessThanEqual(b, c));
}

// run: test_ivec4_less_equal_in_expression() == bvec4(true, false, false, true)
