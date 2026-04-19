// test run
// compile-opt(q32.div, reciprocal)
// @ignore(float_mode=f32)
//
// Divisor == 0 must not trap; matches __lp_lpir_fdiv_q32 saturation policy
// (same as saturating div): +/0 -> MAX_FIXED, -/0 -> MIN_FIXED, 0/0 -> 0.

float test_q32fast_recip_div_pos_by_zero() {
    float a = 1.0;
    float b = 0.0;
    return a / b;
}

// run: test_q32fast_recip_div_pos_by_zero() ~= 32768.0 (tolerance: 0.02)

float test_q32fast_recip_div_neg_by_zero() {
    float a = -1.0;
    float b = 0.0;
    return a / b;
}

// run: test_q32fast_recip_div_neg_by_zero() ~= -32768.0 (tolerance: 0.01)

float test_q32fast_recip_div_zero_by_zero() {
    float a = 0.0;
    float b = 0.0;
    return a / b;
}

// run: test_q32fast_recip_div_zero_by_zero() ~= 0.0
