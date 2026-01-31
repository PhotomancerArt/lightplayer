// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec2/fn-less-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Less Than: lessThan(ivec2, ivec2) -> bvec2 (component-wise)
// ============================================================================

bvec2 test_ivec2_less_than_mixed() {
    // Function lessThan() returns bvec2 (component-wise comparison)
    ivec2 a = ivec2(5, 8);
    ivec2 b = ivec2(7, 6);
    return lessThan(a, b);
}

// run: test_ivec2_less_than_mixed() == bvec2(true, false)

bvec2 test_ivec2_less_than_all_true() {
    ivec2 a = ivec2(1, 2);
    ivec2 b = ivec2(5, 6);
    return lessThan(a, b);
}

// run: test_ivec2_less_than_all_true() == bvec2(true, true)

bvec2 test_ivec2_less_than_all_false() {
    ivec2 a = ivec2(5, 6);
    ivec2 b = ivec2(1, 2);
    return lessThan(a, b);
}

// run: test_ivec2_less_than_all_false() == bvec2(false, false)

bvec2 test_ivec2_less_than_equal() {
    ivec2 a = ivec2(5, 5);
    ivec2 b = ivec2(5, 6);
    return lessThan(a, b);
}

// run: test_ivec2_less_than_equal() == bvec2(false, true)

bvec2 test_ivec2_less_than_negative() {
    ivec2 a = ivec2(-5, -7);
    ivec2 b = ivec2(-1, -3);
    return lessThan(a, b);
}

// run: test_ivec2_less_than_negative() == bvec2(true, true)

bvec2 test_ivec2_less_than_zero() {
    ivec2 a = ivec2(0, 1);
    ivec2 b = ivec2(1, 0);
    return lessThan(a, b);
}

// run: test_ivec2_less_than_zero() == bvec2(true, false)

bvec2 test_ivec2_less_than_variables() {
    ivec2 a = ivec2(10, 15);
    ivec2 b = ivec2(12, 10);
    return lessThan(a, b);
}

// run: test_ivec2_less_than_variables() == bvec2(true, false)

bvec2 test_ivec2_less_than_expressions() {
    return lessThan(ivec2(3, 7), ivec2(5, 5));
}

// run: test_ivec2_less_than_expressions() == bvec2(true, false)

bvec2 test_ivec2_less_than_in_expression() {
    ivec2 a = ivec2(1, 5);
    ivec2 b = ivec2(2, 3);
    ivec2 c = ivec2(3, 7);
    // Use equal() for component-wise comparison of bvec2 values
    // lessThan(a, b) = (true,false)
    // lessThan(b, c) = (true,true)
    // equal(lessThan(a, b), lessThan(b, c)) = (true,false)
    return equal(lessThan(a, b), lessThan(b, c));
}

// run: test_ivec2_less_than_in_expression() == bvec2(true, false)
