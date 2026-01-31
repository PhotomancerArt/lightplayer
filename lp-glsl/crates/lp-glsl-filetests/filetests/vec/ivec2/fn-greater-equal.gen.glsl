// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec2/fn-greater-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than Equal: greaterThanEqual(ivec2, ivec2) -> bvec2 (component-wise)
// ============================================================================

bvec2 test_ivec2_greater_equal_mixed() {
    // Function greaterThanEqual() returns bvec2 (component-wise comparison)
    ivec2 a = ivec2(7, 6);
    ivec2 b = ivec2(5, 8);
    return greaterThanEqual(a, b);
}

// run: test_ivec2_greater_equal_mixed() == bvec2(true, false)

bvec2 test_ivec2_greater_equal_all_true() {
    ivec2 a = ivec2(5, 6);
    ivec2 b = ivec2(1, 2);
    return greaterThanEqual(a, b);
}

// run: test_ivec2_greater_equal_all_true() == bvec2(true, true)

bvec2 test_ivec2_greater_equal_all_false() {
    ivec2 a = ivec2(1, 2);
    ivec2 b = ivec2(5, 6);
    return greaterThanEqual(a, b);
}

// run: test_ivec2_greater_equal_all_false() == bvec2(false, false)

bvec2 test_ivec2_greater_equal_equal() {
    ivec2 a = ivec2(5, 5);
    ivec2 b = ivec2(5, 5);
    return greaterThanEqual(a, b);
}

// run: test_ivec2_greater_equal_equal() == bvec2(true, true)

bvec2 test_ivec2_greater_equal_mixed_equal() {
    ivec2 a = ivec2(5, 6);
    ivec2 b = ivec2(5, 5);
    return greaterThanEqual(a, b);
}

// run: test_ivec2_greater_equal_mixed_equal() == bvec2(true, true)

bvec2 test_ivec2_greater_equal_negative() {
    ivec2 a = ivec2(-1, -3);
    ivec2 b = ivec2(-5, -2);
    return greaterThanEqual(a, b);
}

// run: test_ivec2_greater_equal_negative() == bvec2(true, false)

bvec2 test_ivec2_greater_equal_zero() {
    ivec2 a = ivec2(1, 0);
    ivec2 b = ivec2(0, 1);
    return greaterThanEqual(a, b);
}

// run: test_ivec2_greater_equal_zero() == bvec2(true, false)

bvec2 test_ivec2_greater_equal_variables() {
    ivec2 a = ivec2(12, 10);
    ivec2 b = ivec2(10, 15);
    return greaterThanEqual(a, b);
}

// run: test_ivec2_greater_equal_variables() == bvec2(true, false)

bvec2 test_ivec2_greater_equal_expressions() {
    return greaterThanEqual(ivec2(5, 5), ivec2(3, 7));
}

// run: test_ivec2_greater_equal_expressions() == bvec2(true, false)

bvec2 test_ivec2_greater_equal_in_expression() {
    ivec2 a = ivec2(3, 7);
    ivec2 b = ivec2(2, 3);
    ivec2 c = ivec2(1, 5);
    // Use equal() for component-wise comparison of bvec2 values
    // greaterThanEqual(a, b) = (true,true)
    // greaterThanEqual(b, c) = (true,false)
    // equal(greaterThanEqual(a, b), greaterThanEqual(b, c)) = (true,false)
    return equal(greaterThanEqual(a, b), greaterThanEqual(b, c));
}

// run: test_ivec2_greater_equal_in_expression() == bvec2(true, false)
