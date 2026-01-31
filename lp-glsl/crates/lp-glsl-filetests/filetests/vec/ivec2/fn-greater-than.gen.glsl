// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec2/fn-greater-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than: greaterThan(ivec2, ivec2) -> bvec2 (component-wise)
// ============================================================================

bvec2 test_ivec2_greater_than_mixed() {
    // Function greaterThan() returns bvec2 (component-wise comparison)
    ivec2 a = ivec2(7, 6);
    ivec2 b = ivec2(5, 8);
    return greaterThan(a, b);
}

// run: test_ivec2_greater_than_mixed() == bvec2(true, false)

bvec2 test_ivec2_greater_than_all_true() {
    ivec2 a = ivec2(5, 6);
    ivec2 b = ivec2(1, 2);
    return greaterThan(a, b);
}

// run: test_ivec2_greater_than_all_true() == bvec2(true, true)

bvec2 test_ivec2_greater_than_all_false() {
    ivec2 a = ivec2(1, 2);
    ivec2 b = ivec2(5, 6);
    return greaterThan(a, b);
}

// run: test_ivec2_greater_than_all_false() == bvec2(false, false)

bvec2 test_ivec2_greater_than_equal() {
    ivec2 a = ivec2(5, 6);
    ivec2 b = ivec2(5, 5);
    return greaterThan(a, b);
}

// run: test_ivec2_greater_than_equal() == bvec2(false, true)

bvec2 test_ivec2_greater_than_negative() {
    ivec2 a = ivec2(-1, -3);
    ivec2 b = ivec2(-5, -7);
    return greaterThan(a, b);
}

// run: test_ivec2_greater_than_negative() == bvec2(true, true)

bvec2 test_ivec2_greater_than_zero() {
    ivec2 a = ivec2(1, 0);
    ivec2 b = ivec2(0, 1);
    return greaterThan(a, b);
}

// run: test_ivec2_greater_than_zero() == bvec2(true, false)

bvec2 test_ivec2_greater_than_variables() {
    ivec2 a = ivec2(12, 10);
    ivec2 b = ivec2(10, 15);
    return greaterThan(a, b);
}

// run: test_ivec2_greater_than_variables() == bvec2(true, false)

bvec2 test_ivec2_greater_than_expressions() {
    return greaterThan(ivec2(5, 5), ivec2(3, 7));
}

// run: test_ivec2_greater_than_expressions() == bvec2(true, false)

bvec2 test_ivec2_greater_than_in_expression() {
    ivec2 a = ivec2(3, 7);
    ivec2 b = ivec2(2, 3);
    ivec2 c = ivec2(1, 5);
    // Use equal() for component-wise comparison of bvec2 values
    // greaterThan(a, b) = (true,true)
    // greaterThan(b, c) = (true,false)
    // equal(greaterThan(a, b), greaterThan(b, c)) = (true,false)
    return equal(greaterThan(a, b), greaterThan(b, c));
}

// run: test_ivec2_greater_than_in_expression() == bvec2(true, false)
