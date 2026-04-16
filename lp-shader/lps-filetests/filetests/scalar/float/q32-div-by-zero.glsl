// test run
// Q32 div-by-zero saturation (docs/design/q32.md). Native f32 uses IEEE Inf/NaN instead.
// @ignore(float_mode=f32)

// ============================================================================
// Division by zero — Q32 only
// ============================================================================

float test_q32_div_pos_by_zero() {
    float a = 1.0;
    float b = 0.0;
    return a / b;
}

// run: test_q32_div_pos_by_zero() ~= 32768.0 (tolerance: 0.02)

float test_q32_div_neg_by_zero() {
    float a = -1.0;
    float b = 0.0;
    return a / b;
}

// run: test_q32_div_neg_by_zero() ~= -32768.0 (tolerance: 0.01)

float test_q32_div_zero_by_zero() {
    float a = 0.0;
    float b = 0.0;
    return a / b;
}

// run: test_q32_div_zero_by_zero() ~= 0.0
