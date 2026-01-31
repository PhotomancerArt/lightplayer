// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec4/fn-greater-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than: greaterThan(vec4, vec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_vec4_greater_than_mixed() {
    // Function greaterThan() returns bvec4 (component-wise comparison)
    vec4 a = vec4(7.0, 6.0, 9.0, 2.0);
    vec4 b = vec4(5.0, 8.0, 7.0, 4.0);
    return greaterThan(a, b);
}

// run: test_vec4_greater_than_mixed() == bvec4(true, false, true, false)

bvec4 test_vec4_greater_than_all_true() {
    vec4 a = vec4(5.0, 6.0, 7.0, 8.0);
    vec4 b = vec4(1.0, 2.0, 3.0, 4.0);
    return greaterThan(a, b);
}

// run: test_vec4_greater_than_all_true() == bvec4(true, true, true, true)

bvec4 test_vec4_greater_than_all_false() {
    vec4 a = vec4(1.0, 2.0, 3.0, 4.0);
    vec4 b = vec4(5.0, 6.0, 7.0, 8.0);
    return greaterThan(a, b);
}

// run: test_vec4_greater_than_all_false() == bvec4(false, false, false, false)

bvec4 test_vec4_greater_than_equal() {
    vec4 a = vec4(5.0, 6.0, 7.0, 8.0);
    vec4 b = vec4(5.0, 5.0, 8.0, 7.0);
    return greaterThan(a, b);
}

// run: test_vec4_greater_than_equal() == bvec4(false, true, false, true)

bvec4 test_vec4_greater_than_negative() {
    vec4 a = vec4(-1.0, -3.0, 2.0, -5.0);
    vec4 b = vec4(-5.0, -7.0, 0.0, -8.0);
    return greaterThan(a, b);
}

// run: test_vec4_greater_than_negative() == bvec4(true, true, true, true)

bvec4 test_vec4_greater_than_zero() {
    vec4 a = vec4(1.0, 0.0, 3.0, -1.0);
    vec4 b = vec4(0.0, 1.0, 2.0, 0.0);
    return greaterThan(a, b);
}

// run: test_vec4_greater_than_zero() == bvec4(true, false, true, false)

bvec4 test_vec4_greater_than_variables() {
    vec4 a = vec4(12.0, 10.0, 8.0, 6.0);
    vec4 b = vec4(10.0, 15.0, 9.0, 7.0);
    return greaterThan(a, b);
}

// run: test_vec4_greater_than_variables() == bvec4(true, false, false, false)

bvec4 test_vec4_greater_than_expressions() {
    return greaterThan(vec4(5.0, 5.0, 6.0, 3.0), vec4(3.0, 7.0, 8.0, 4.0));
}

// run: test_vec4_greater_than_expressions() == bvec4(true, false, false, false)

bvec4 test_vec4_greater_than_in_expression() {
    vec4 a = vec4(3.0, 7.0, 5.0, 9.0);
    vec4 b = vec4(2.0, 3.0, 6.0, 8.0);
    vec4 c = vec4(1.0, 5.0, 4.0, 7.0);
    // Use equal() for component-wise comparison of bvec4 values
    // greaterThan(a, b) = (true,true,false,true)
    // greaterThan(b, c) = (true,false,true,true)
    // equal(greaterThan(a, b), greaterThan(b, c)) = (true,false,false,true)
    return equal(greaterThan(a, b), greaterThan(b, c));
}

// run: test_vec4_greater_than_in_expression() == bvec4(true, false, false, true)
