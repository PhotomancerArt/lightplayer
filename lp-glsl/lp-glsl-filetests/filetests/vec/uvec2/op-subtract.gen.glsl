// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec2/op-subtract --write
//
// test run

// ============================================================================
// Subtract: uvec2 - uvec2 -> uvec2 (component-wise)
// ============================================================================

uvec2 test_uvec2_subtract_positive_positive() {
// Subtraction with unsigned vectors (component-wise, no underflow)
uvec2 a = uvec2(50u, 40u);
uvec2 b = uvec2(10u, 15u);
return a - b;
}

// run: test_uvec2_subtract_positive_positive() == uvec2(40u, 25u)

uvec2 test_uvec2_subtract_zero() {
uvec2 a = uvec2(42u, 17u);
uvec2 b = uvec2(0u, 0u);
return a - b;
}

// run: test_uvec2_subtract_zero() == uvec2(42u, 17u)

uvec2 test_uvec2_subtract_variables() {
uvec2 a = uvec2(50u, 40u);
uvec2 b = uvec2(10u, 5u);
return a - b;
}

// run: test_uvec2_subtract_variables() == uvec2(40u, 35u)

uvec2 test_uvec2_subtract_expressions() {
return uvec2(80u, 60u) - uvec2(30u, 20u);
}

// run: test_uvec2_subtract_expressions() == uvec2(50u, 40u)

uvec2 test_uvec2_subtract_in_assignment() {
uvec2 result = uvec2(100u, 80u);
result = result - uvec2(30u, 25u);
return result;
}

// run: test_uvec2_subtract_in_assignment() == uvec2(70u, 55u)

uvec2 test_uvec2_subtract_large_numbers() {
uvec2 a = uvec2(5000u, 4000u);
uvec2 b = uvec2(1000u, 3500u);
return a - b;
}

// run: test_uvec2_subtract_large_numbers() == uvec2(4000u, 500u)

uvec2 test_uvec2_subtract_max_values() {
uvec2 a = uvec2(4294967295u, 4294967294u);
uvec2 b = uvec2(1u, 1u);
return a - b;
}

// run: test_uvec2_subtract_max_values() == uvec2(4294967294u, 4294967293u)

uvec2 test_uvec2_subtract_mixed_components() {
uvec2 a = uvec2(300u, 125u);
uvec2 b = uvec2(200u, 75u);
return a - b;
}

// run: test_uvec2_subtract_mixed_components() == uvec2(100u, 50u)

