// test run
// compile-opt(q32.div, reciprocal)
// @ignore(float_mode=f32)
//
// Reciprocal division (__lp_lpir_fdiv_recip_q32 path). For normal inputs the
// result is close to true a/b; exact Q16.16 value depends on the algorithm.

float test_q32fast_div_recip_ten_over_three() {
    float a = 10.0;
    float b = 3.0;
    return a / b;
}

// run: test_q32fast_div_recip_ten_over_three() ~= 3.3331298828125 (tolerance: 0.001)
