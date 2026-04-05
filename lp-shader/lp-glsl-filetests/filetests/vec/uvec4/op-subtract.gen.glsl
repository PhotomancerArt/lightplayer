// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec4/op-subtract --write
//
// test run

// ============================================================================
// Subtract: uvec4 - uvec4 -> uvec4 (component-wise)
// ============================================================================

uvec4 test_uvec4_subtract_positive_positive() {
// Subtraction with unsigned vectors (component-wise, no underflow)
uvec4 a = uvec4(50u, 40u, 30u, 20u);
uvec4 b = uvec4(10u, 15u, 5u, 8u);
return a - b;
}

// run: test_uvec4_subtract_positive_positive() == uvec4(40u, 25u, 25u, 12u)

uvec4 test_uvec4_subtract_zero() {
uvec4 a = uvec4(42u, 17u, 23u, 8u);
uvec4 b = uvec4(0u, 0u, 0u, 0u);
return a - b;
}

// run: test_uvec4_subtract_zero() == uvec4(42u, 17u, 23u, 8u)

uvec4 test_uvec4_subtract_variables() {
uvec4 a = uvec4(50u, 40u, 35u, 30u);
uvec4 b = uvec4(10u, 5u, 12u, 3u);
return a - b;
}

// run: test_uvec4_subtract_variables() == uvec4(40u, 35u, 23u, 27u)

uvec4 test_uvec4_subtract_expressions() {
return uvec4(80u, 60u, 50u, 40u) - uvec4(30u, 20u, 10u, 5u);
}

// run: test_uvec4_subtract_expressions() == uvec4(50u, 40u, 40u, 35u)

uvec4 test_uvec4_subtract_in_assignment() {
uvec4 result = uvec4(100u, 80u, 60u, 40u);
result = result - uvec4(30u, 25u, 10u, 5u);
return result;
}

// run: test_uvec4_subtract_in_assignment() == uvec4(70u, 55u, 50u, 35u)

uvec4 test_uvec4_subtract_large_numbers() {
uvec4 a = uvec4(5000u, 4000u, 3000u, 2000u);
uvec4 b = uvec4(1000u, 500u, 200u, 1500u);
return a - b;
}

// run: test_uvec4_subtract_large_numbers() == uvec4(4000u, 3500u, 2800u, 500u)

uvec4 test_uvec4_subtract_max_values() {
uvec4 a = uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u);
uvec4 b = uvec4(1u, 1u, 1u, 1u);
return a - b;
}

// run: test_uvec4_subtract_max_values() == uvec4(4294967294u, 4294967293u, 4294967292u, 4294967291u)

uvec4 test_uvec4_subtract_mixed_components() {
uvec4 a = uvec4(300u, 125u, 225u, 75u);
uvec4 b = uvec4(200u, 75u, 150u, 50u);
return a - b;
}

// run: test_uvec4_subtract_mixed_components() == uvec4(100u, 50u, 75u, 25u)

