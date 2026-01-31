// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec3/fn-greater-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than: greaterThan(ivec3, ivec3) -> bvec3 (component-wise)
// ============================================================================

bvec3 test_ivec3_greater_than_mixed() {
    // Function greaterThan() returns bvec3 (component-wise comparison)
    ivec3 a = ivec3(7, 6, 9);
    ivec3 b = ivec3(5, 8, 7);
    return greaterThan(a, b);
}

// run: test_ivec3_greater_than_mixed() == bvec3(true, false, true)

bvec3 test_ivec3_greater_than_all_true() {
    ivec3 a = ivec3(5, 6, 7);
    ivec3 b = ivec3(1, 2, 3);
    return greaterThan(a, b);
}

// run: test_ivec3_greater_than_all_true() == bvec3(true, true, true)

bvec3 test_ivec3_greater_than_all_false() {
    ivec3 a = ivec3(1, 2, 3);
    ivec3 b = ivec3(5, 6, 7);
    return greaterThan(a, b);
}

// run: test_ivec3_greater_than_all_false() == bvec3(false, false, false)

bvec3 test_ivec3_greater_than_equal() {
    ivec3 a = ivec3(5, 6, 7);
    ivec3 b = ivec3(5, 5, 8);
    return greaterThan(a, b);
}

// run: test_ivec3_greater_than_equal() == bvec3(false, true, false)

bvec3 test_ivec3_greater_than_negative() {
    ivec3 a = ivec3(-1, -3, 2);
    ivec3 b = ivec3(-5, -7, 0);
    return greaterThan(a, b);
}

// run: test_ivec3_greater_than_negative() == bvec3(true, true, true)

bvec3 test_ivec3_greater_than_zero() {
    ivec3 a = ivec3(1, 0, 3);
    ivec3 b = ivec3(0, 1, 2);
    return greaterThan(a, b);
}

// run: test_ivec3_greater_than_zero() == bvec3(true, false, true)

bvec3 test_ivec3_greater_than_variables() {
    ivec3 a = ivec3(12, 10, 8);
    ivec3 b = ivec3(10, 15, 9);
    return greaterThan(a, b);
}

// run: test_ivec3_greater_than_variables() == bvec3(true, false, false)

bvec3 test_ivec3_greater_than_expressions() {
    return greaterThan(ivec3(5, 5, 6), ivec3(3, 7, 8));
}

// run: test_ivec3_greater_than_expressions() == bvec3(true, false, false)

bvec3 test_ivec3_greater_than_in_expression() {
    ivec3 a = ivec3(3, 7, 5);
    ivec3 b = ivec3(2, 3, 6);
    ivec3 c = ivec3(1, 5, 4);
    // Use equal() for component-wise comparison of bvec3 values
    // greaterThan(a, b) = (true,true,false)
    // greaterThan(b, c) = (true,false,true)
    // equal(greaterThan(a, b), greaterThan(b, c)) = (true,false,false)
    return equal(greaterThan(a, b), greaterThan(b, c));
}

// run: test_ivec3_greater_than_in_expression() == bvec3(true, false, false)
