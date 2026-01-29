// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-filetests-gen vec/vec2/fn-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: equal(vec2, vec2) -> bvec2 (component-wise)
// ============================================================================

bvec2 test_vec2_equal_function_mixed() {
vec2 a = vec2(5.0, 3.0);
vec2 b = vec2(5.0, 4.0);
// Function equal() returns bvec2 (component-wise equality)
return equal(a, b);
}

// run: test_vec2_equal_function_mixed() == bvec2(true, false)

bvec2 test_vec2_equal_function_all_true() {
vec2 a = vec2(10.0, 20.0);
vec2 b = vec2(10.0, 20.0);
return equal(a, b);
}

// run: test_vec2_equal_function_all_true() == bvec2(true, true)

bvec2 test_vec2_equal_function_all_false() {
vec2 a = vec2(5.0, 3.0);
vec2 b = vec2(2.0, 4.0);
return equal(a, b);
}

// run: test_vec2_equal_function_all_false() == bvec2(false, false)

bvec2 test_vec2_equal_function_zero() {
vec2 a = vec2(0.0, 5.0);
vec2 b = vec2(0.0, 3.0);
return equal(a, b);
}

// run: test_vec2_equal_function_zero() == bvec2(true, false)

bvec2 test_vec2_equal_function_negative() {
vec2 a = vec2(-5.0, -3.0);
vec2 b = vec2(-5.0, -4.0);
return equal(a, b);
}

// run: test_vec2_equal_function_negative() == bvec2(true, false)

bvec2 test_vec2_equal_function_variables() {
vec2 a = vec2(8.0, 12.0);
vec2 b = vec2(8.0, 10.0);
return equal(a, b);
}

// run: test_vec2_equal_function_variables() == bvec2(true, false)

bvec2 test_vec2_equal_function_expressions() {
return equal(vec2(2.0, 5.0), vec2(2.0, 4.0));
}

// run: test_vec2_equal_function_expressions() == bvec2(true, false)

bvec2 test_vec2_equal_function_in_expression() {
vec2 a = vec2(1.0, 3.0);
vec2 b = vec2(1.0, 4.0);
vec2 c = vec2(2.0, 3.0);
// Use equal() for component-wise comparison of bvec2 values
// equal(a, b) = (true,false)
// equal(b, c) = (false,false)
// equal(equal(a, b), equal(b, c)) = (false,false)
return equal(equal(a, b), equal(b, c));
}

// run: test_vec2_equal_function_in_expression() == bvec2(false, false) [expect-fail]
