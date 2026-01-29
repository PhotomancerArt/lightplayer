// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-filetests-gen vec/ivec2/fn-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: equal(ivec2, ivec2) -> bvec2 (component-wise)
// ============================================================================

bvec2 test_ivec2_equal_function_mixed() {
ivec2 a = ivec2(5, 3);
ivec2 b = ivec2(5, 4);
// Function equal() returns bvec2 (component-wise equality)
return equal(a, b);
}

// run: test_ivec2_equal_function_mixed() == bvec2(true, false)

bvec2 test_ivec2_equal_function_all_true() {
ivec2 a = ivec2(10, 20);
ivec2 b = ivec2(10, 20);
return equal(a, b);
}

// run: test_ivec2_equal_function_all_true() == bvec2(true, true)

bvec2 test_ivec2_equal_function_all_false() {
ivec2 a = ivec2(5, 3);
ivec2 b = ivec2(2, 4);
return equal(a, b);
}

// run: test_ivec2_equal_function_all_false() == bvec2(false, false)

bvec2 test_ivec2_equal_function_zero() {
ivec2 a = ivec2(0, 5);
ivec2 b = ivec2(0, 3);
return equal(a, b);
}

// run: test_ivec2_equal_function_zero() == bvec2(true, false)

bvec2 test_ivec2_equal_function_negative() {
ivec2 a = ivec2(-5, -3);
ivec2 b = ivec2(-5, -4);
return equal(a, b);
}

// run: test_ivec2_equal_function_negative() == bvec2(true, false)

bvec2 test_ivec2_equal_function_variables() {
ivec2 a = ivec2(8, 12);
ivec2 b = ivec2(8, 10);
return equal(a, b);
}

// run: test_ivec2_equal_function_variables() == bvec2(true, false)

bvec2 test_ivec2_equal_function_expressions() {
return equal(ivec2(2, 5), ivec2(2, 4));
}

// run: test_ivec2_equal_function_expressions() == bvec2(true, false)

bvec2 test_ivec2_equal_function_in_expression() {
ivec2 a = ivec2(1, 3);
ivec2 b = ivec2(1, 4);
ivec2 c = ivec2(2, 3);
// Use equal() for component-wise comparison of bvec2 values
// equal(a, b) = (true,false)
// equal(b, c) = (false,false)
// equal(equal(a, b), equal(b, c)) = (false,false)
return equal(equal(a, b), equal(b, c));
}

// run: test_ivec2_equal_function_in_expression() == bvec2(false, false) [expect-fail]
