// test run
// compile-opt(q32.mul, wrapping)
// @ignore(float_mode=f32)
//
// Wrapping multiply: ((a*b) >> 16) as i32. Large products wrap instead of
// saturating. Here a = b = 32767.0 (raw 2147418112); ((a*b)>>16) as i32 = 65536 = 1.0.
//
// Cranelift RV32 does not yet apply `compile-opt(q32.mul, wrapping)` (would saturate to max).

float test_q32fast_wrap_mul_large_square() {
    float a = 32767.0;
    float b = 32767.0;
    return a * b;
}

// per-mode: the f32 channel asserts IEEE f32 results; Q32 keeps its saturation/wrapping expectation (M6 triage).
// @unsupported(rv32c.q32)
// run[q32]: test_q32fast_wrap_mul_large_square() ~= 1.0 (tolerance: 0.02)
