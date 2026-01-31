// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec2/fn-greater-than --write
//
// test run
// target riscv32.q32

// ============================================================================
// Greater Than: greaterThan(vec2, vec2) -> bvec2 (component-wise)
// ============================================================================

bvec2 test_vec2_greater_than_mixed() {
    // Function greaterThan() returns bvec2 (component-wise comparison)
    vec2 a = vec2(7.0, 6.0);
    vec2 b = vec2(5.0, 8.0);
    return greaterThan(a, b);
}

// run: test_vec2_greater_than_mixed() == bvec2(true, false)

bvec2 test_vec2_greater_than_all_true() {
    vec2 a = vec2(5.0, 6.0);
    vec2 b = vec2(1.0, 2.0);
    return greaterThan(a, b);
}

// run: test_vec2_greater_than_all_true() == bvec2(true, true)

bvec2 test_vec2_greater_than_all_false() {
    vec2 a = vec2(1.0, 2.0);
    vec2 b = vec2(5.0, 6.0);
    return greaterThan(a, b);
}

// run: test_vec2_greater_than_all_false() == bvec2(false, false)

bvec2 test_vec2_greater_than_equal() {
    vec2 a = vec2(5.0, 6.0);
    vec2 b = vec2(5.0, 5.0);
    return greaterThan(a, b);
}

// run: test_vec2_greater_than_equal() == bvec2(false, true)

bvec2 test_vec2_greater_than_negative() {
    vec2 a = vec2(-1.0, -3.0);
    vec2 b = vec2(-5.0, -7.0);
    return greaterThan(a, b);
}

// run: test_vec2_greater_than_negative() == bvec2(true, true)

bvec2 test_vec2_greater_than_zero() {
    vec2 a = vec2(1.0, 0.0);
    vec2 b = vec2(0.0, 1.0);
    return greaterThan(a, b);
}

// run: test_vec2_greater_than_zero() == bvec2(true, false)

bvec2 test_vec2_greater_than_variables() {
    vec2 a = vec2(12.0, 10.0);
    vec2 b = vec2(10.0, 15.0);
    return greaterThan(a, b);
}

// run: test_vec2_greater_than_variables() == bvec2(true, false)

bvec2 test_vec2_greater_than_expressions() {
    return greaterThan(vec2(5.0, 5.0), vec2(3.0, 7.0));
}

// run: test_vec2_greater_than_expressions() == bvec2(true, false)

bvec2 test_vec2_greater_than_in_expression() {
    vec2 a = vec2(3.0, 7.0);
    vec2 b = vec2(2.0, 3.0);
    vec2 c = vec2(1.0, 5.0);
    // Use equal() for component-wise comparison of bvec2 values
    // greaterThan(a, b) = (true,true)
    // greaterThan(b, c) = (true,false)
    // equal(greaterThan(a, b), greaterThan(b, c)) = (true,false)
    return equal(greaterThan(a, b), greaterThan(b, c));
}

// run: test_vec2_greater_than_in_expression() == bvec2(true, false)
