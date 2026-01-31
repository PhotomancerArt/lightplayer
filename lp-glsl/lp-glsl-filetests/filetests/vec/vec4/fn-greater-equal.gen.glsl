// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec4/fn-greater-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than Equal: greaterThanEqual(vec4, vec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_vec4_greater_equal_mixed() {
    // Function greaterThanEqual() returns bvec4 (component-wise comparison)
    vec4 a = vec4(7.0, 6.0, 9.0, 2.0);
    vec4 b = vec4(5.0, 8.0, 7.0, 4.0);
    return greaterThanEqual(a, b);
}

// run: test_vec4_greater_equal_mixed() == bvec4(true, false, true, false)

bvec4 test_vec4_greater_equal_all_true() {
    vec4 a = vec4(5.0, 6.0, 7.0, 8.0);
    vec4 b = vec4(1.0, 2.0, 3.0, 4.0);
    return greaterThanEqual(a, b);
}

// run: test_vec4_greater_equal_all_true() == bvec4(true, true, true, true)

bvec4 test_vec4_greater_equal_all_false() {
    vec4 a = vec4(1.0, 2.0, 3.0, 4.0);
    vec4 b = vec4(5.0, 6.0, 7.0, 8.0);
    return greaterThanEqual(a, b);
}

// run: test_vec4_greater_equal_all_false() == bvec4(false, false, false, false)

bvec4 test_vec4_greater_equal_equal() {
    vec4 a = vec4(5.0, 5.0, 5.0, 5.0);
    vec4 b = vec4(5.0, 5.0, 5.0, 5.0);
    return greaterThanEqual(a, b);
}

// run: test_vec4_greater_equal_equal() == bvec4(true, true, true, true)

bvec4 test_vec4_greater_equal_mixed_equal() {
    vec4 a = vec4(5.0, 6.0, 7.0, 8.0);
    vec4 b = vec4(5.0, 5.0, 8.0, 7.0);
    return greaterThanEqual(a, b);
}

// run: test_vec4_greater_equal_mixed_equal() == bvec4(true, true, false, true)

bvec4 test_vec4_greater_equal_negative() {
    vec4 a = vec4(-1.0, -3.0, 2.0, -5.0);
    vec4 b = vec4(-5.0, -2.0, 1.0, -7.0);
    return greaterThanEqual(a, b);
}

// run: test_vec4_greater_equal_negative() == bvec4(true, false, true, true)

bvec4 test_vec4_greater_equal_zero() {
    vec4 a = vec4(1.0, 0.0, 3.0, -1.0);
    vec4 b = vec4(0.0, 1.0, 2.0, 0.0);
    return greaterThanEqual(a, b);
}

// run: test_vec4_greater_equal_zero() == bvec4(true, false, true, false)

bvec4 test_vec4_greater_equal_variables() {
    vec4 a = vec4(12.0, 10.0, 8.0, 6.0);
    vec4 b = vec4(10.0, 15.0, 8.0, 5.0);
    return greaterThanEqual(a, b);
}

// run: test_vec4_greater_equal_variables() == bvec4(true, false, true, true)

bvec4 test_vec4_greater_equal_expressions() {
    return greaterThanEqual(vec4(5.0, 5.0, 6.0, 3.0), vec4(3.0, 7.0, 6.0, 4.0));
}

// run: test_vec4_greater_equal_expressions() == bvec4(true, false, true, false)

bvec4 test_vec4_greater_equal_in_expression() {
    vec4 a = vec4(3.0, 7.0, 5.0, 9.0);
    vec4 b = vec4(2.0, 3.0, 6.0, 8.0);
    vec4 c = vec4(1.0, 5.0, 4.0, 7.0);
    // Use equal() for component-wise comparison of bvec4 values
    // greaterThanEqual(a, b) = (true,true,false,true)
    // greaterThanEqual(b, c) = (true,false,true,true)
    // equal(greaterThanEqual(a, b), greaterThanEqual(b, c)) = (true,false,false,true)
    return equal(greaterThanEqual(a, b), greaterThanEqual(b, c));
}

// run: test_vec4_greater_equal_in_expression() == bvec4(true, false, false, true)
