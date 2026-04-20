// test run
// compile-opt(q32.add_sub, wrapping)
// @ignore(float_mode=f32)
//
// Wrapping add/sub uses i32.add / i32.sub on Q16.16 raw values. Values that
// overflow the representable range wrap (mod 2^32) instead of saturating to
// MAX_FIXED like the default q32.add_sub=saturating path.
//
// Constants: MAX_FIXED raw = 0x7FFF_FFFF; 32767.0 and 1.0 encode to
// 2147418112 and 65536. Their sum overflows i32 to -2147483648 (MIN_FIXED) =
// -32768.0 as float.

float test_q32fast_wrap_add_max_plus_one() {
    float a = 32767.0;
    float b = 1.0;
    return a + b;
}

// Cranelift RV32 path does not yet thread `CompilerConfig::q32` (saturating add here).
// @unsupported(rv32c.q32)
// run: test_q32fast_wrap_add_max_plus_one() ~= -32768.0 (tolerance: 0.02)

float test_q32fast_wrap_sub_one_minus_max() {
    float a = 1.0;
    float b = 32767.0;
    return a - b;
}

// run: test_q32fast_wrap_sub_one_minus_max() ~= -32766.0 (tolerance: 0.02)
