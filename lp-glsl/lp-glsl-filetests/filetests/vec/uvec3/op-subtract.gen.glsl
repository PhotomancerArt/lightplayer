// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec3/op-subtract --write
//
// test run

// ============================================================================
// Subtract: uvec3 - uvec3 -> uvec3 (component-wise)
// ============================================================================

uvec3 test_uvec3_subtract_positive_positive() {
// Subtraction with unsigned vectors (component-wise, no underflow)
uvec3 a = uvec3(50u, 40u, 30u);
uvec3 b = uvec3(10u, 15u, 5u);
return a - b;
}

// run: test_uvec3_subtract_positive_positive() == uvec3(40u, 25u, 25u)

uvec3 test_uvec3_subtract_zero() {
uvec3 a = uvec3(42u, 17u, 23u);
uvec3 b = uvec3(0u, 0u, 0u);
return a - b;
}

// run: test_uvec3_subtract_zero() == uvec3(42u, 17u, 23u)

uvec3 test_uvec3_subtract_variables() {
uvec3 a = uvec3(50u, 40u, 35u);
uvec3 b = uvec3(10u, 5u, 12u);
return a - b;
}

// run: test_uvec3_subtract_variables() == uvec3(40u, 35u, 23u)

uvec3 test_uvec3_subtract_expressions() {
return uvec3(80u, 60u, 50u) - uvec3(30u, 20u, 10u);
}

// run: test_uvec3_subtract_expressions() == uvec3(50u, 40u, 40u)

uvec3 test_uvec3_subtract_in_assignment() {
uvec3 result = uvec3(100u, 80u, 60u);
result = result - uvec3(30u, 25u, 10u);
return result;
}

// run: test_uvec3_subtract_in_assignment() == uvec3(70u, 55u, 50u)

uvec3 test_uvec3_subtract_large_numbers() {
uvec3 a = uvec3(5000u, 4000u, 3000u);
uvec3 b = uvec3(1000u, 500u, 200u);
return a - b;
}

// run: test_uvec3_subtract_large_numbers() == uvec3(4000u, 3500u, 2800u)

uvec3 test_uvec3_subtract_max_values() {
uvec3 a = uvec3(4294967295u, 4294967294u, 4294967293u);
uvec3 b = uvec3(1u, 1u, 1u);
return a - b;
}

// run: test_uvec3_subtract_max_values() == uvec3(4294967294u, 4294967293u, 4294967292u)

uvec3 test_uvec3_subtract_mixed_components() {
uvec3 a = uvec3(300u, 125u, 225u);
uvec3 b = uvec3(200u, 75u, 150u);
return a - b;
}

// run: test_uvec3_subtract_mixed_components() == uvec3(100u, 50u, 75u)

