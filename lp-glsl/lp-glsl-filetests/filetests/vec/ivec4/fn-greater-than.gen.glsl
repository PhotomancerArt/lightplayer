// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/fn-greater-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than: greaterThan(ivec4, ivec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_ivec4_greater_than_mixed() {
    // Function greaterThan() returns bvec4 (component-wise comparison)
    ivec4 a = ivec4(7, 6, 9, 2);
    ivec4 b = ivec4(5, 8, 7, 4);
    return greaterThan(a, b);
}

// run: test_ivec4_greater_than_mixed() == bvec4(true, false, true, false)

bvec4 test_ivec4_greater_than_all_true() {
    ivec4 a = ivec4(5, 6, 7, 8);
    ivec4 b = ivec4(1, 2, 3, 4);
    return greaterThan(a, b);
}

// run: test_ivec4_greater_than_all_true() == bvec4(true, true, true, true)

bvec4 test_ivec4_greater_than_all_false() {
    ivec4 a = ivec4(1, 2, 3, 4);
    ivec4 b = ivec4(5, 6, 7, 8);
    return greaterThan(a, b);
}

// run: test_ivec4_greater_than_all_false() == bvec4(false, false, false, false)

bvec4 test_ivec4_greater_than_equal() {
    ivec4 a = ivec4(5, 6, 7, 8);
    ivec4 b = ivec4(5, 5, 8, 7);
    return greaterThan(a, b);
}

// run: test_ivec4_greater_than_equal() == bvec4(false, true, false, true)

bvec4 test_ivec4_greater_than_negative() {
    ivec4 a = ivec4(-1, -3, 2, -5);
    ivec4 b = ivec4(-5, -7, 0, -8);
    return greaterThan(a, b);
}

// run: test_ivec4_greater_than_negative() == bvec4(true, true, true, true)

bvec4 test_ivec4_greater_than_zero() {
    ivec4 a = ivec4(1, 0, 3, -1);
    ivec4 b = ivec4(0, 1, 2, 0);
    return greaterThan(a, b);
}

// run: test_ivec4_greater_than_zero() == bvec4(true, false, true, false)

bvec4 test_ivec4_greater_than_variables() {
    ivec4 a = ivec4(12, 10, 8, 6);
    ivec4 b = ivec4(10, 15, 9, 7);
    return greaterThan(a, b);
}

// run: test_ivec4_greater_than_variables() == bvec4(true, false, false, false)

bvec4 test_ivec4_greater_than_expressions() {
    return greaterThan(ivec4(5, 5, 6, 3), ivec4(3, 7, 8, 4));
}

// run: test_ivec4_greater_than_expressions() == bvec4(true, false, false, false)

bvec4 test_ivec4_greater_than_in_expression() {
    ivec4 a = ivec4(3, 7, 5, 9);
    ivec4 b = ivec4(2, 3, 6, 8);
    ivec4 c = ivec4(1, 5, 4, 7);
    // Use equal() for component-wise comparison of bvec4 values
    // greaterThan(a, b) = (true,true,false,true)
    // greaterThan(b, c) = (true,false,true,true)
    // equal(greaterThan(a, b), greaterThan(b, c)) = (true,false,false,true)
    return equal(greaterThan(a, b), greaterThan(b, c));
}

// run: test_ivec4_greater_than_in_expression() == bvec4(true, false, false, true)
