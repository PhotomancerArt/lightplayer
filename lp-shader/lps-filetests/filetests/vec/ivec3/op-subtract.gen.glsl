// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lps-filetests-gen-app vec/ivec3/op-subtract --write
//
// test run

// ============================================================================
// Subtract: ivec3 - ivec3 -> ivec3 (component-wise)
// ============================================================================

ivec3 test_ivec3_subtract_positive_positive() {
    // Subtraction with positive vectors (component-wise)
    ivec3 a = ivec3(5, 3, 2);
    ivec3 b = ivec3(2, 4, 1);
    return a - b;
}

// run: test_ivec3_subtract_positive_positive() == ivec3(3, -1, 1)

ivec3 test_ivec3_subtract_positive_negative() {
    ivec3 a = ivec3(10, 8, 5);
    ivec3 b = ivec3(-4, -2, -1);
    return a - b;
}

// run: test_ivec3_subtract_positive_negative() == ivec3(14, 10, 6)

ivec3 test_ivec3_subtract_negative_negative() {
    ivec3 a = ivec3(-3, -7, -2);
    ivec3 b = ivec3(-2, -1, -3);
    return a - b;
}

// run: test_ivec3_subtract_negative_negative() == ivec3(-1, -6, 1)

ivec3 test_ivec3_subtract_zero() {
    ivec3 a = ivec3(42, 17, 23);
    ivec3 b = ivec3(0, 0, 0);
    return a - b;
}

// run: test_ivec3_subtract_zero() == ivec3(42, 17, 23)

ivec3 test_ivec3_subtract_variables() {
    ivec3 a = ivec3(15, 10, 5);
    ivec3 b = ivec3(27, 5, 12);
    return a - b;
}

// run: test_ivec3_subtract_variables() == ivec3(-12, 5, -7)

ivec3 test_ivec3_subtract_expressions() {
    return ivec3(8, 4, 6) - ivec3(6, 2, 3);
}

// run: test_ivec3_subtract_expressions() == ivec3(2, 2, 3)

ivec3 test_ivec3_subtract_in_assignment() {
    ivec3 result = ivec3(5, 3, 2);
    result = result - ivec3(10, 7, 8);
    return result;
}

// run: test_ivec3_subtract_in_assignment() == ivec3(-5, -4, -6)

ivec3 test_ivec3_subtract_large_numbers() {
    ivec3 a = ivec3(5000, 4000, 3000);
    ivec3 b = ivec3(1000, 500, 200);
    return a - b;
}

// run: test_ivec3_subtract_large_numbers() == ivec3(4000, 3500, 2800)

ivec3 test_ivec3_subtract_mixed_components() {
    ivec3 a = ivec3(1, -2, 3);
    ivec3 b = ivec3(-3, 4, -1);
    return a - b;
}

// run: test_ivec3_subtract_mixed_components() == ivec3(4, -6, 4)

