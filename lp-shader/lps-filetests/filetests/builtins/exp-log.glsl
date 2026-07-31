// test run

layout(binding = 0) uniform float u_runtime_zero;

float rt(float x) { return x + u_runtime_zero; }

// ============================================================================
// log(): Natural logarithm function
// log(x) returns ln(x)
// Undefined if x <= 0
// ============================================================================

float test_log_one() {
    // log(1) should be 0
    return log(rt(1.0));
}

// wasm.f32: builtin import has no f32 implementation — only Q32 builtin ids
// resolve, so the import cannot be lowered in f32 mode. Unblocks with M5.
// @unimplemented(wasm.f32)
// run: test_log_one() ~= 0.0

float test_log_e() {
    // log(e) should be 1
    return log(rt(2.718281828459045));
}

// @unimplemented(wasm.f32)
// run: test_log_e() ~= 1.0

float test_log_two() {
    // log(2) should be ln(2) ≈ 0.6931471805599453
    return log(rt(2.0));
}

// @unimplemented(wasm.f32)
// run: test_log_two() ~= 0.6931471805599453

float test_log_ten() {
    // log(10) should be ln(10) ≈ 2.302585092994046
    return log(rt(10.0));
}

// @unimplemented(wasm.f32)
// run: test_log_ten() ~= 2.302585092994046

float test_log_half() {
    // log(0.5) should be ln(0.5) ≈ -0.6931471805599453
    return log(rt(0.5));
}

// @unimplemented(wasm.f32)
// run: test_log_half() ~= -0.6931471805599453

float test_log_sqrt_e() {
    // log(√e) should be 0.5
    return log(rt(1.6487212711532444));
}

// @unimplemented(wasm.f32)
// run: test_log_sqrt_e() ~= 0.5

vec2 test_log_vec2() {
    // Test with vec2
    return log(vec2(rt(1.0), rt(2.718281828459045)));
}

// @unimplemented(wasm.f32)
// run: test_log_vec2() ~= vec2(0.0, 1.0)

vec3 test_log_vec3() {
    // Test with vec3
    return log(vec3(rt(1.0), rt(2.0), rt(10.0)));
}

// @unimplemented(wasm.f32)
// run: test_log_vec3() ~= vec3(0.0, 0.6931471805599453, 2.302585092994046)

vec4 test_log_vec4() {
    // Test with vec4
    return log(vec4(rt(1.0), rt(2.0), rt(0.5), rt(0.1)));
}

// @unimplemented(wasm.f32)
// run: test_log_vec4() ~= vec4(0.0, 0.6931471805599453, -0.6931471805599453, -2.302585092994046)




