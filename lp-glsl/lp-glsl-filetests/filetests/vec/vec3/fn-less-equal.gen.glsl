// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec3/fn-greater-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Less Than Equal: lessThanEqual(vec3, vec3) -> bvec3 (component-wise)
// ============================================================================

bvec3 test_vec3_less_equal_mixed() {
    // Function lessThanEqual() returns bvec3 (component-wise comparison)
    vec3 a = vec3(5.0, 8.0, 7.0);
    vec3 b = vec3(7.0, 6.0, 9.0);
    return lessThanEqual(a, b);
}

// run: test_vec3_less_equal_mixed() == bvec3(true, false, true)

bvec3 test_vec3_less_equal_all_true() {
    vec3 a = vec3(1.0, 2.0, 3.0);
    vec3 b = vec3(5.0, 6.0, 7.0);
    return lessThanEqual(a, b);
}

// run: test_vec3_less_equal_all_true() == bvec3(true, true, true)

bvec3 test_vec3_less_equal_all_false() {
    vec3 a = vec3(5.0, 6.0, 7.0);
    vec3 b = vec3(1.0, 2.0, 3.0);
    return lessThanEqual(a, b);
}

// run: test_vec3_less_equal_all_false() == bvec3(false, false, false)

bvec3 test_vec3_less_equal_equal() {
    vec3 a = vec3(5.0, 5.0, 5.0);
    vec3 b = vec3(5.0, 5.0, 5.0);
    return lessThanEqual(a, b);
}

// run: test_vec3_less_equal_equal() == bvec3(true, true, true)

bvec3 test_vec3_less_equal_mixed_equal() {
    vec3 a = vec3(5.0, 6.0, 7.0);
    vec3 b = vec3(5.0, 5.0, 8.0);
    return lessThanEqual(a, b);
}

// run: test_vec3_less_equal_mixed_equal() == bvec3(true, false, true)

bvec3 test_vec3_less_equal_negative() {
    vec3 a = vec3(-5.0, -2.0, 1.0);
    vec3 b = vec3(-1.0, -3.0, 2.0);
    return lessThanEqual(a, b);
}

// run: test_vec3_less_equal_negative() == bvec3(true, false, true)

bvec3 test_vec3_less_equal_zero() {
    vec3 a = vec3(0.0, 1.0, 2.0);
    vec3 b = vec3(1.0, 0.0, 3.0);
    return lessThanEqual(a, b);
}

// run: test_vec3_less_equal_zero() == bvec3(true, false, true)

bvec3 test_vec3_less_equal_variables() {
    vec3 a = vec3(10.0, 15.0, 8.0);
    vec3 b = vec3(12.0, 10.0, 12.0);
    return lessThanEqual(a, b);
}

// run: test_vec3_less_equal_variables() == bvec3(true, false, true)

bvec3 test_vec3_less_equal_expressions() {
    return lessThanEqual(vec3(3.0, 7.0, 2.0), vec3(5.0, 5.0, 4.0));
}

// run: test_vec3_less_equal_expressions() == bvec3(true, false, true)

bvec3 test_vec3_less_equal_in_expression() {
    vec3 a = vec3(1.0, 5.0, 4.0);
    vec3 b = vec3(2.0, 3.0, 6.0);
    vec3 c = vec3(3.0, 7.0, 5.0);
    // Use equal() for component-wise comparison of bvec3 values
    // lessThanEqual(a, b) = (true,false,true)
    // lessThanEqual(b, c) = (true,true,false)
    // equal(lessThanEqual(a, b), lessThanEqual(b, c)) = (true,false,false)
    return equal(lessThanEqual(a, b), lessThanEqual(b, c));
}

// run: test_vec3_less_equal_in_expression() == bvec3(true, false, false)
