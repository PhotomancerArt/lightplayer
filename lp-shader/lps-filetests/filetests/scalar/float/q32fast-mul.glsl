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

// @unsupported(rv32c.q32)
// run: test_q32fast_wrap_mul_large_square() ~= 1.0 (tolerance: 0.02)
