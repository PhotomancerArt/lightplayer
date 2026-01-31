// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec3/fn-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: equal(vec3, vec3) -> bvec3 (component-wise)
// ============================================================================

bvec3 test_vec3_equal_function_mixed() {
    vec3 a = vec3(5.0, 3.0, 7.0);
    vec3 b = vec3(5.0, 4.0, 7.0);
    // Function equal() returns bvec3 (component-wise equality)
    return equal(a, b);
}

// run: test_vec3_equal_function_mixed() == bvec3(true, false, true)

bvec3 test_vec3_equal_function_all_true() {
    vec3 a = vec3(10.0, 20.0, 30.0);
    vec3 b = vec3(10.0, 20.0, 30.0);
    return equal(a, b);
}

// run: test_vec3_equal_function_all_true() == bvec3(true, true, true)

bvec3 test_vec3_equal_function_all_false() {
    vec3 a = vec3(5.0, 3.0, 7.0);
    vec3 b = vec3(2.0, 4.0, 1.0);
    return equal(a, b);
}

// run: test_vec3_equal_function_all_false() == bvec3(false, false, false)

bvec3 test_vec3_equal_function_zero() {
    vec3 a = vec3(0.0, 5.0, 0.0);
    vec3 b = vec3(0.0, 3.0, 1.0);
    return equal(a, b);
}

// run: test_vec3_equal_function_zero() == bvec3(true, false, false)

bvec3 test_vec3_equal_function_negative() {
    vec3 a = vec3(-5.0, -3.0, -7.0);
    vec3 b = vec3(-5.0, -4.0, -7.0);
    return equal(a, b);
}

// run: test_vec3_equal_function_negative() == bvec3(true, false, true)

bvec3 test_vec3_equal_function_variables() {
    vec3 a = vec3(8.0, 12.0, 6.0);
    vec3 b = vec3(8.0, 10.0, 7.0);
    return equal(a, b);
}

// run: test_vec3_equal_function_variables() == bvec3(true, false, false)

bvec3 test_vec3_equal_function_expressions() {
    return equal(vec3(2.0, 5.0, 3.0), vec3(2.0, 4.0, 8.0));
}

// run: test_vec3_equal_function_expressions() == bvec3(true, false, false)

bvec3 test_vec3_equal_function_in_expression() {
    vec3 a = vec3(1.0, 3.0, 5.0);
    vec3 b = vec3(1.0, 4.0, 5.0);
    vec3 c = vec3(2.0, 3.0, 5.0);
    // Use equal() for component-wise comparison of bvec3 values
    // equal(a, b) = (true,false,true)
    // equal(b, c) = (false,false,true)
    // equal(equal(a, b), equal(b, c)) = (false,true,true)
    return equal(equal(a, b), equal(b, c));
}

// run: test_vec3_equal_function_in_expression() == bvec3(false, true, true)
