// test run
// target riscv32.q32

// ============================================================================
// Precision and rounding tests
// Testing precision limits and rounding behavior
// ============================================================================

float test_round_half_up() {
    // round(2.5) - implementation-defined behavior
    return round(2.5);
}

// run: test_round_half_up() ~= 3.0 [expect-fail]

float test_round_half_down() {
    // round(3.5) - implementation-defined behavior
    return round(3.5);
}

// run: test_round_half_down() ~= 4.0 [expect-fail]

float test_roundeven_half_up() {
    // roundEven(2.5) - should round to even (3.0)
    return roundEven(2.5);
}

// run: test_roundeven_half_up() ~= 3.0 [expect-fail]

float test_roundeven_half_down() {
    // roundEven(3.5) - should round to even (4.0)
    return roundEven(3.5);
}

// run: test_roundeven_half_down() ~= 4.0 [expect-fail]

float test_precision_loss() {
    // Test precision loss with very small additions
    return 1.0 + 1e-10;
}

// run: test_precision_loss() ~= 1.0 [expect-fail]

float test_large_number_precision() {
    // Test precision with large numbers
    return 1e10 + 1.0;
}

// run: test_large_number_precision() ~= 10000000000.0 [expect-fail]

float test_subnormal() {
    // Test with very small numbers
    return 1e-40 + 1e-40;
}

// run: test_subnormal() ~= 0.0 [expect-fail]

vec2 test_vec_precision() {
    // Test precision with vectors
    return vec2(1.0 + 1e-10, 1e10 + 1.0);
}

// run: test_vec_precision() ~= vec2(1.0, 10000000000.0) [expect-fail]

float test_fma_precision() {
    // Test if fma provides better precision than separate multiply-add
    return fma(1.0000001, 1.0000001, -1.0000001);
}

// run: test_fma_precision() ~= 0.0000001020000001 [expect-fail]




