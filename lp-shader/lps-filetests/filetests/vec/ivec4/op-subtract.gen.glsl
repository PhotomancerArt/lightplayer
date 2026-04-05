// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lps-filetests-gen-app vec/ivec4/op-subtract --write
//
// test run

// ============================================================================
// Subtract: ivec4 - ivec4 -> ivec4 (component-wise)
// ============================================================================

ivec4 test_ivec4_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    ivec4 a = ivec4(5, 3, 2, 1);
    ivec4 b = ivec4(2, 4, 1, 3);
    return a - b;
}

// run: test_ivec4_subtract_positive_positive() == ivec4(3, -1, 1, -2)

ivec4 test_ivec4_subtract_positive_negative() {
    ivec4 a = ivec4(10, 8, 5, 3);
    ivec4 b = ivec4(-4, -2, -1, -3);
    return a - b;
}

// run: test_ivec4_subtract_positive_negative() == ivec4(14, 10, 6, 6)

ivec4 test_ivec4_subtract_negative_negative() {
    ivec4 a = ivec4(-3, -7, -2, -5);
    ivec4 b = ivec4(-2, -1, -3, -1);
    return a - b;
}

// run: test_ivec4_subtract_negative_negative() == ivec4(-1, -6, 1, -4)

ivec4 test_ivec4_subtract_zero() {
    ivec4 a = ivec4(42, 17, 23, 8);
    ivec4 b = ivec4(0, 0, 0, 0);
    return a - b;
}

// run: test_ivec4_subtract_zero() == ivec4(42, 17, 23, 8)

ivec4 test_ivec4_subtract_variables() {
    ivec4 a = ivec4(15, 10, 5, 12);
    ivec4 b = ivec4(27, 5, 12, 3);
    return a - b;
}

// run: test_ivec4_subtract_variables() == ivec4(-12, 5, -7, 9)

ivec4 test_ivec4_subtract_expressions() {
    return ivec4(8, 4, 6, 2) - ivec4(6, 2, 3, 4);
}

// run: test_ivec4_subtract_expressions() == ivec4(2, 2, 3, -2)

ivec4 test_ivec4_subtract_in_assignment() {
    ivec4 result = ivec4(5, 3, 2, 1);
    result = result - ivec4(10, 7, 8, 9);
    return result;
}

// run: test_ivec4_subtract_in_assignment() == ivec4(-5, -4, -6, -8)

ivec4 test_ivec4_subtract_large_numbers() {
    ivec4 a = ivec4(5000, 4000, 3000, 2000);
    ivec4 b = ivec4(1000, 500, 200, 1500);
    return a - b;
}

// run: test_ivec4_subtract_large_numbers() == ivec4(4000, 3500, 2800, 500)

ivec4 test_ivec4_subtract_mixed_components() {
    ivec4 a = ivec4(1, -2, 3, -4);
    ivec4 b = ivec4(-3, 4, -1, 2);
    return a - b;
}

// run: test_ivec4_subtract_mixed_components() == ivec4(4, -6, 4, -6)

