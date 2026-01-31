// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec3/fn-less-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Less Than: lessThan(vec3, vec3) -> bvec3 (component-wise)
// ============================================================================

bvec3 test_vec3_less_than_mixed() {
    // Function lessThan() returns bvec3 (component-wise comparison)
    vec3 a = vec3(5.0, 8.0, 7.0);
    vec3 b = vec3(7.0, 6.0, 9.0);
    return lessThan(a, b);
}

// run: test_vec3_less_than_mixed() == bvec3(true, false, true)

bvec3 test_vec3_less_than_all_true() {
    vec3 a = vec3(1.0, 2.0, 3.0);
    vec3 b = vec3(5.0, 6.0, 7.0);
    return lessThan(a, b);
}

// run: test_vec3_less_than_all_true() == bvec3(true, true, true)

bvec3 test_vec3_less_than_all_false() {
    vec3 a = vec3(5.0, 6.0, 7.0);
    vec3 b = vec3(1.0, 2.0, 3.0);
    return lessThan(a, b);
}

// run: test_vec3_less_than_all_false() == bvec3(false, false, false)

bvec3 test_vec3_less_than_equal() {
    vec3 a = vec3(5.0, 5.0, 5.0);
    vec3 b = vec3(5.0, 6.0, 4.0);
    return lessThan(a, b);
}

// run: test_vec3_less_than_equal() == bvec3(false, true, false)

bvec3 test_vec3_less_than_negative() {
    vec3 a = vec3(-5.0, -7.0, 0.0);
    vec3 b = vec3(-1.0, -3.0, 2.0);
    return lessThan(a, b);
}

// run: test_vec3_less_than_negative() == bvec3(true, true, true)

bvec3 test_vec3_less_than_zero() {
    vec3 a = vec3(0.0, 1.0, 2.0);
    vec3 b = vec3(1.0, 0.0, 3.0);
    return lessThan(a, b);
}

// run: test_vec3_less_than_zero() == bvec3(true, false, true)

bvec3 test_vec3_less_than_variables() {
    vec3 a = vec3(10.0, 15.0, 8.0);
    vec3 b = vec3(12.0, 10.0, 12.0);
    return lessThan(a, b);
}

// run: test_vec3_less_than_variables() == bvec3(true, false, true)

bvec3 test_vec3_less_than_expressions() {
    return lessThan(vec3(3.0, 7.0, 2.0), vec3(5.0, 5.0, 4.0));
}

// run: test_vec3_less_than_expressions() == bvec3(true, false, true)

bvec3 test_vec3_less_than_in_expression() {
    vec3 a = vec3(1.0, 5.0, 4.0);
    vec3 b = vec3(2.0, 3.0, 6.0);
    vec3 c = vec3(3.0, 7.0, 5.0);
    // Use equal() for component-wise comparison of bvec3 values
    // lessThan(a, b) = (true,false,true)
    // lessThan(b, c) = (true,true,false)
    // equal(lessThan(a, b), lessThan(b, c)) = (true,false,false)
    return equal(lessThan(a, b), lessThan(b, c));
}

// run: test_vec3_less_than_in_expression() == bvec3(true, false, false)
