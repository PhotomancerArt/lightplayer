// test run
// target riscv32.q32

// ============================================================================
// Trigonometric domain error tests
// Testing undefined behavior for trigonometric functions
// ============================================================================

// Note: These tests document expected undefined behavior
// The actual results may vary between implementations

float test_asin_domain_over() {
    // asin(1.5) - |x| > 1, undefined behavior
    return asin(1.5);
}

// run: test_asin_domain_over() ~= 0.0

float test_asin_domain_under() {
    // asin(-1.5) - |x| > 1, undefined behavior
    return asin(-1.5);
}

// run: test_asin_domain_under() ~= 0.0

float test_acos_domain_over() {
    // acos(1.5) - |x| > 1, undefined behavior
    return acos(1.5);
}

// run: test_acos_domain_over() ~= 0.0 [expect-fail]

float test_acos_domain_under() {
    // acos(-1.5) - |x| > 1, undefined behavior
    return acos(-1.5);
}

// run: test_acos_domain_under() ~= 0.0 [expect-fail]

float test_atan2_zero_zero() {
    // atan(0, 0) - both x and y are 0, undefined behavior
    return atan(0.0, 0.0);
}

// run: test_atan2_zero_zero() ~= 0.0 [expect-fail]

float test_acosh_domain_under() {
    // acosh(0.5) - x < 1, undefined behavior
    return acosh(0.5);
}

// run: test_acosh_domain_under() ~= 0.0

float test_atanh_domain_over() {
    // atanh(1.5) - |x| >= 1, undefined behavior
    return atanh(1.5);
}

// run: test_atanh_domain_over() ~= 0.0

float test_atanh_domain_under() {
    // atanh(-1.5) - |x| >= 1, undefined behavior
    return atanh(-1.5);
}

// run: test_atanh_domain_under() ~= 0.0

float test_atanh_domain_one() {
    // atanh(1.0) - |x| >= 1, undefined behavior
    return atanh(1.0);
}

// run: test_atanh_domain_one() ~= 0.0

float test_atanh_domain_neg_one() {
    // atanh(-1.0) - |x| >= 1, undefined behavior
    return atanh(-1.0);
}

// run: test_atanh_domain_neg_one() ~= 0.0




