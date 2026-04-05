// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec2/op-subtract --write
//
// test run

// ============================================================================
// Subtract: ivec2 - ivec2 -> ivec2 (component-wise)
// ============================================================================

ivec2 test_ivec2_subtract_positive_positive() {
// Subtraction with positive vectors (component-wise)
ivec2 a = ivec2(5, 3);
ivec2 b = ivec2(2, 4);
return a - b;
}

// run: test_ivec2_subtract_positive_positive() == ivec2(3, -1)

ivec2 test_ivec2_subtract_positive_negative() {
ivec2 a = ivec2(10, 8);
ivec2 b = ivec2(-4, -2);
return a - b;
}

// run: test_ivec2_subtract_positive_negative() == ivec2(14, 10)

ivec2 test_ivec2_subtract_negative_negative() {
ivec2 a = ivec2(-3, -7);
ivec2 b = ivec2(-2, -1);
return a - b;
}

// run: test_ivec2_subtract_negative_negative() == ivec2(-1, -6)

ivec2 test_ivec2_subtract_zero() {
ivec2 a = ivec2(42, 17);
ivec2 b = ivec2(0, 0);
return a - b;
}

// run: test_ivec2_subtract_zero() == ivec2(42, 17)

ivec2 test_ivec2_subtract_variables() {
ivec2 a = ivec2(15, 10);
ivec2 b = ivec2(27, 5);
return a - b;
}

// run: test_ivec2_subtract_variables() == ivec2(-12, 5)

ivec2 test_ivec2_subtract_expressions() {
return ivec2(8, 4) - ivec2(6, 2);
}

// run: test_ivec2_subtract_expressions() == ivec2(2, 2)

ivec2 test_ivec2_subtract_in_assignment() {
ivec2 result = ivec2(5, 3);
result = result - ivec2(10, 7);
return result;
}

// run: test_ivec2_subtract_in_assignment() == ivec2(-5, -4)

ivec2 test_ivec2_subtract_large_numbers() {
ivec2 a = ivec2(5000, 4000);
ivec2 b = ivec2(1000, 3500);
return a - b;
}

// run: test_ivec2_subtract_large_numbers() == ivec2(4000, 500)

ivec2 test_ivec2_subtract_mixed_components() {
ivec2 a = ivec2(1, -2);
ivec2 b = ivec2(-3, 4);
return a - b;
}

// run: test_ivec2_subtract_mixed_components() == ivec2(4, -6)

