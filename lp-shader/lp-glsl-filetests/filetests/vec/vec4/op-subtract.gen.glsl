// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec4/op-subtract --write
//
// test run

// ============================================================================
// Subtract: vec4 - vec4 -> vec4 (component-wise)
// ============================================================================

vec4 test_vec4_subtract_positive_positive() {
// Subtraction with positive vectors (component-wise)
vec4 a = vec4(5.0, 3.0, 2.0, 1.0);
vec4 b = vec4(2.0, 4.0, 1.0, 3.0);
return a - b;
}

// run: test_vec4_subtract_positive_positive() ~= vec4(3.0, -1.0, 1.0, -2.0)

vec4 test_vec4_subtract_positive_negative() {
vec4 a = vec4(10.0, 8.0, 5.0, 3.0);
vec4 b = vec4(-4.0, -2.0, -1.0, -3.0);
return a - b;
}

// run: test_vec4_subtract_positive_negative() ~= vec4(14.0, 10.0, 6.0, 6.0)

vec4 test_vec4_subtract_negative_negative() {
vec4 a = vec4(-3.0, -7.0, -2.0, -5.0);
vec4 b = vec4(-2.0, -1.0, -3.0, -1.0);
return a - b;
}

// run: test_vec4_subtract_negative_negative() ~= vec4(-1.0, -6.0, 1.0, -4.0)

vec4 test_vec4_subtract_zero() {
vec4 a = vec4(42.0, 17.0, 23.0, 8.0);
vec4 b = vec4(0.0, 0.0, 0.0, 0.0);
return a - b;
}

// run: test_vec4_subtract_zero() ~= vec4(42.0, 17.0, 23.0, 8.0)

vec4 test_vec4_subtract_variables() {
vec4 a = vec4(15.0, 10.0, 5.0, 12.0);
vec4 b = vec4(27.0, 5.0, 12.0, 3.0);
return a - b;
}

// run: test_vec4_subtract_variables() ~= vec4(-12.0, 5.0, -7.0, 9.0)

vec4 test_vec4_subtract_expressions() {
return vec4(8.0, 4.0, 6.0, 2.0) - vec4(6.0, 2.0, 3.0, 4.0);
}

// run: test_vec4_subtract_expressions() ~= vec4(2.0, 2.0, 3.0, -2.0)

vec4 test_vec4_subtract_in_assignment() {
vec4 result = vec4(5.0, 3.0, 2.0, 1.0);
result = result - vec4(10.0, 7.0, 8.0, 9.0);
return result;
}

// run: test_vec4_subtract_in_assignment() ~= vec4(-5.0, -4.0, -6.0, -8.0)

vec4 test_vec4_subtract_large_numbers() {
vec4 a = vec4(5000.0, 4000.0, 3000.0, 2000.0);
vec4 b = vec4(1000.0, 500.0, 200.0, 1500.0);
return a - b;
}

// run: test_vec4_subtract_large_numbers() ~= vec4(4000.0, 3500.0, 2800.0, 500.0)

vec4 test_vec4_subtract_mixed_components() {
vec4 a = vec4(1.0, -2.0, 3.0, -4.0);
vec4 b = vec4(-3.0, 4.0, -1.0, 2.0);
return a - b;
}

// run: test_vec4_subtract_mixed_components() ~= vec4(4.0, -6.0, 4.0, -6.0)

vec4 test_vec4_subtract_fractions() {
vec4 a = vec4(1.5, 2.25, 3.75, 0.5);
vec4 b = vec4(0.5, 1.75, 0.25, 1.5);
return a - b;
}

// run: test_vec4_subtract_fractions() ~= vec4(1.0, 0.5, 3.5, -1.0)
