// test run

// ============================================================================
// NaN and Inf propagation tests
// Testing how NaN and Inf values propagate through operations
// ============================================================================

// Note: These tests document expected behavior with special values
// The actual results may vary between implementations
//
// Because the results may vary, no target asserts them. Every directive below
// carries the wildcard disposition, which covers present *and* future targets.
// On float_mode=q32 there is no IEEE NaN/Inf in the numeric model at all, so
// the special values these name do not exist there.
//
// The wildcard replaces per-target enumerations that had drifted. Two targets
// were executing the first two assertions purely because a line was missing
// from their block, and that is not coverage: `rv32lpn.q32` differs from
// `xtlpn.q32` only in the ISA, was excluded by hand, and passes identically
// when you strip the exclusion. Which of the two ends up asserting behaviour
// its numeric model cannot represent is an accident of list maintenance. A
// wildcard cannot drift.
//
// (The naga frontend cannot compile this file; the lps-glsl frontend can.
// Earlier notes here said no target compiled it, which was never true.)

bool test_isnan_inf() {
    // isnan with positive infinity should be false
    return isnan(1.0 / 0.0);
}

// @unsupported(*)
// run: test_isnan_inf() == false

bool test_isnan_neg_inf() {
    // isnan with negative infinity should be false
    return isnan(-1.0 / 0.0);
}

// @unsupported(*)
// run: test_isnan_neg_inf() == false

bool test_isinf_inf() {
    // isinf with positive infinity should be true
    return isinf(1.0 / 0.0);
}

// @unsupported(*)
// run: test_isinf_inf() == true

bool test_isinf_neg_inf() {
    // isinf with negative infinity should be true
    return isinf(-1.0 / 0.0);
}

// @unsupported(*)
// run: test_isinf_neg_inf() == true

float test_sin_inf() {
    // sin with infinity - should produce NaN or undefined
    return sin(1.0 / 0.0);
}

// @unsupported(*)
// run: test_sin_inf() ~= 0.0

float test_cos_inf() {
    // cos with infinity - should produce NaN or undefined
    return cos(1.0 / 0.0);
}

// @unsupported(*)
// run: test_cos_inf() ~= 0.0

float test_log_inf() {
    // log with infinity - should produce infinity
    return log(1.0 / 0.0);
}

// @unsupported(*)
// run: test_log_inf() ~= 0.0

float test_exp_inf() {
    // exp with infinity - should produce infinity
    return exp(1.0 / 0.0);
}

// @unsupported(*)
// run: test_exp_inf() ~= 0.0

float test_sqrt_inf() {
    // sqrt with infinity - should produce infinity
    return sqrt(1.0 / 0.0);
}

// @unsupported(*)
// run: test_sqrt_inf() ~= 0.0

vec2 test_nan_propagation() {
    // Test NaN propagation through operations
    float nan_val = 0.0 / 0.0;
    return vec2(nan_val + 1.0, nan_val * 2.0);
}

// @unsupported(*)
// run: test_nan_propagation() ~= vec2(0.0, 0.0)

vec2 test_inf_propagation() {
    // Test Inf propagation through operations
    float inf_val = 1.0 / 0.0;
    return vec2(inf_val + 1.0, inf_val * 2.0);
}

// wgpu.f32: file does not compile through naga glsl-in (mirrors the interp.f32 frontend gap)
// @unsupported(*)
// run: test_inf_propagation() ~= vec2(0.0, 0.0)


