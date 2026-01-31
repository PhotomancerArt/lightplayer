// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/fn-less-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Less Than: lessThan(ivec4, ivec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_ivec4_less_than_mixed() {
    // Function lessThan() returns bvec4 (component-wise comparison)
    ivec4 a = ivec4(5, 8, 7, 4);
    ivec4 b = ivec4(7, 6, 9, 2);
    return lessThan(a, b);
}

// run: test_ivec4_less_than_mixed() == bvec4(true, false, true, false)

bvec4 test_ivec4_less_than_all_true() {
    ivec4 a = ivec4(1, 2, 3, 4);
    ivec4 b = ivec4(5, 6, 7, 8);
    return lessThan(a, b);
}

// run: test_ivec4_less_than_all_true() == bvec4(true, true, true, true)

bvec4 test_ivec4_less_than_all_false() {
    ivec4 a = ivec4(5, 6, 7, 8);
    ivec4 b = ivec4(1, 2, 3, 4);
    return lessThan(a, b);
}

// run: test_ivec4_less_than_all_false() == bvec4(false, false, false, false)

bvec4 test_ivec4_less_than_equal() {
    ivec4 a = ivec4(5, 5, 5, 5);
    ivec4 b = ivec4(5, 6, 4, 7);
    return lessThan(a, b);
}

// run: test_ivec4_less_than_equal() == bvec4(false, true, false, true)

bvec4 test_ivec4_less_than_negative() {
    ivec4 a = ivec4(-5, -7, 0, -8);
    ivec4 b = ivec4(-1, -3, 2, -5);
    return lessThan(a, b);
}

// run: test_ivec4_less_than_negative() == bvec4(true, true, true, true)

bvec4 test_ivec4_less_than_zero() {
    ivec4 a = ivec4(0, 1, 2, 0);
    ivec4 b = ivec4(1, 0, 3, -1);
    return lessThan(a, b);
}

// run: test_ivec4_less_than_zero() == bvec4(true, false, true, false)

bvec4 test_ivec4_less_than_variables() {
    ivec4 a = ivec4(10, 15, 8, 12);
    ivec4 b = ivec4(12, 10, 12, 8);
    return lessThan(a, b);
}

// run: test_ivec4_less_than_variables() == bvec4(true, false, true, false)

bvec4 test_ivec4_less_than_expressions() {
    return lessThan(ivec4(3, 7, 2, 9), ivec4(5, 5, 4, 8));
}

// run: test_ivec4_less_than_expressions() == bvec4(true, false, true, false)

bvec4 test_ivec4_less_than_in_expression() {
    ivec4 a = ivec4(1, 5, 4, 7);
    ivec4 b = ivec4(2, 3, 6, 8);
    ivec4 c = ivec4(3, 7, 5, 9);
    // Use equal() for component-wise comparison of bvec4 values
    // lessThan(a, b) = (true,false,true,true)
    // lessThan(b, c) = (true,true,false,true)
    // equal(lessThan(a, b), lessThan(b, c)) = (true,false,false,true)
    return equal(lessThan(a, b), lessThan(b, c));
}

// run: test_ivec4_less_than_in_expression() == bvec4(true, false, false, true)
