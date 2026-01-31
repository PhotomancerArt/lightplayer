// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec4/fn-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: equal(vec4, vec4) -> bvec4 (component-wise)
// ============================================================================

bvec4 test_vec4_equal_function_mixed() {
    vec4 a = vec4(5.0, 3.0, 7.0, 2.0);
    vec4 b = vec4(5.0, 4.0, 7.0, 2.0);
    // Function equal() returns bvec4 (component-wise equality)
    return equal(a, b);
}

// run: test_vec4_equal_function_mixed() == bvec4(true, false, true, true)

bvec4 test_vec4_equal_function_all_true() {
    vec4 a = vec4(10.0, 20.0, 30.0, 40.0);
    vec4 b = vec4(10.0, 20.0, 30.0, 40.0);
    return equal(a, b);
}

// run: test_vec4_equal_function_all_true() == bvec4(true, true, true, true)

bvec4 test_vec4_equal_function_all_false() {
    vec4 a = vec4(5.0, 3.0, 7.0, 2.0);
    vec4 b = vec4(2.0, 4.0, 1.0, 3.0);
    return equal(a, b);
}

// run: test_vec4_equal_function_all_false() == bvec4(false, false, false, false)

bvec4 test_vec4_equal_function_zero() {
    vec4 a = vec4(0.0, 5.0, 0.0, 2.0);
    vec4 b = vec4(0.0, 3.0, 1.0, 2.0);
    return equal(a, b);
}

// run: test_vec4_equal_function_zero() == bvec4(true, false, false, true)

bvec4 test_vec4_equal_function_negative() {
    vec4 a = vec4(-5.0, -3.0, -7.0, -2.0);
    vec4 b = vec4(-5.0, -4.0, -7.0, -1.0);
    return equal(a, b);
}

// run: test_vec4_equal_function_negative() == bvec4(true, false, true, false)

bvec4 test_vec4_equal_function_variables() {
    vec4 a = vec4(8.0, 12.0, 6.0, 9.0);
    vec4 b = vec4(8.0, 10.0, 7.0, 9.0);
    return equal(a, b);
}

// run: test_vec4_equal_function_variables() == bvec4(true, false, false, true)

bvec4 test_vec4_equal_function_expressions() {
    return equal(vec4(2.0, 5.0, 3.0, 8.0), vec4(2.0, 4.0, 8.0, 8.0));
}

// run: test_vec4_equal_function_expressions() == bvec4(true, false, false, true)

bvec4 test_vec4_equal_function_in_expression() {
    vec4 a = vec4(1.0, 3.0, 5.0, 7.0);
    vec4 b = vec4(1.0, 4.0, 5.0, 7.0);
    vec4 c = vec4(2.0, 3.0, 5.0, 6.0);
    // Use equal() for component-wise comparison of bvec4 values
    // equal(a, b) = (true,false,true,true)
    // equal(b, c) = (false,false,true,false)
    // equal(equal(a, b), equal(b, c)) = (false,true,true,false)
    return equal(equal(a, b), equal(b, c));
}

// run: test_vec4_equal_function_in_expression() == bvec4(false, true, true, false)
