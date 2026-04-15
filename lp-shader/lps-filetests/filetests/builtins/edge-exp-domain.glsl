// test run

// ============================================================================
// Exponential domain error tests
// Testing undefined behavior for exponential functions
// ============================================================================

// Note: These tests document expected undefined behavior
// The actual results may vary between implementations
// On float_mode=q32 these are skipped (see file-level @unsupported).

float test_pow_negative_base() {
    // pow(-2.0, 3.0) - x < 0, undefined behavior
    return pow(-2.0, 3.0);
}

// @unimplemented(jit.q32)
// @unimplemented(wasm.q32)
// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_pow_negative_base() ~= 0.0

float test_pow_zero_negative_exponent() {
    // pow(0.0, -1.0) - x = 0 and y <= 0, undefined behavior
    return pow(0.0, -1.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_pow_zero_negative_exponent() ~= 0.0

float test_pow_zero_zero() {
    // pow(0.0, 0.0) - x = 0 and y <= 0, undefined behavior
    return pow(0.0, 0.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_pow_zero_zero() ~= 0.0

float test_log_zero() {
    // log(0.0) - x <= 0, undefined behavior
    return log(0.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_log_zero() ~= 0.0

float test_log_negative() {
    // log(-1.0) - x <= 0, undefined behavior
    return log(-1.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_log_negative() ~= 0.0

float test_log2_zero() {
    // log2(0.0) - x <= 0, undefined behavior
    return log2(0.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_log2_zero() ~= 0.0

float test_log2_negative() {
    // log2(-1.0) - x <= 0, undefined behavior
    return log2(-1.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_log2_negative() ~= 0.0

float test_sqrt_negative() {
    // sqrt(-1.0) - x < 0, undefined behavior
    return sqrt(-1.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_sqrt_negative() ~= 0.0

float test_inversesqrt_zero() {
    // inversesqrt(0.0) - x <= 0, undefined behavior
    return inversesqrt(0.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_inversesqrt_zero() ~= 0.0

float test_inversesqrt_negative() {
    // inversesqrt(-1.0) - x <= 0, undefined behavior
    return inversesqrt(-1.0);
}

// @unsupported(jit.q32)
// @unsupported(rv32.q32)
// @unsupported(rv32fa.q32)
// @unsupported(wasm.q32)
// run: test_inversesqrt_negative() ~= 0.0


