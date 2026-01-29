// test run
// target riscv32.q32

// ============================================================================
// NaN and Inf propagation tests
// Testing how NaN and Inf values propagate through operations
// ============================================================================

// Note: These tests document expected behavior with special values
// The actual results may vary between implementations

bool test_isnan_inf() {
    // isnan with positive infinity should be false
    return isnan(1.0 / 0.0);
}

// run: test_isnan_inf() == false

bool test_isnan_neg_inf() {
    // isnan with negative infinity should be false
    return isnan(-1.0 / 0.0);
}

// run: test_isnan_neg_inf() == false

bool test_isinf_inf() {
    // isinf with positive infinity should be true
    return isinf(1.0 / 0.0);
}

// run: test_isinf_inf() == true

bool test_isinf_neg_inf() {
    // isinf with negative infinity should be true
    return isinf(-1.0 / 0.0);
}

// run: test_isinf_neg_inf() == true

float test_sin_inf() {
    // sin with infinity - should produce NaN or undefined
    return sin(1.0 / 0.0);
}

// run: test_sin_inf() ~= 0.0 [expect-fail]

float test_cos_inf() {
    // cos with infinity - should produce NaN or undefined
    return cos(1.0 / 0.0);
}

// run: test_cos_inf() ~= 0.0 [expect-fail]

float test_log_inf() {
    // log with infinity - should produce infinity
    return log(1.0 / 0.0);
}

// run: test_log_inf() ~= 0.0 [expect-fail]

float test_exp_inf() {
    // exp with infinity - should produce infinity
    return exp(1.0 / 0.0);
}

// run: test_exp_inf() ~= 0.0 [expect-fail]

float test_sqrt_inf() {
    // sqrt with infinity - should produce infinity
    return sqrt(1.0 / 0.0);
}

// run: test_sqrt_inf() ~= 0.0 [expect-fail]

vec2 test_nan_propagation() {
    // Test NaN propagation through operations
    float nan_val = 0.0 / 0.0;
    return vec2(nan_val + 1.0, nan_val * 2.0);
}

// run: test_nan_propagation() ~= vec2(0.0, 0.0) [expect-fail]

vec2 test_inf_propagation() {
    // Test Inf propagation through operations
    float inf_val = 1.0 / 0.0;
    return vec2(inf_val + 1.0, inf_val * 2.0);
}

// run: test_inf_propagation() ~= vec2(0.0, 0.0) [expect-fail]




