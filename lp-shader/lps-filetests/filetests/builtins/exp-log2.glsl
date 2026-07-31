// test run

layout(binding = 0) uniform float u_runtime_zero;

float rt(float x) { return x + u_runtime_zero; }

// ============================================================================
// log2(): Base 2 logarithm function
// log2(x) returns log2(x)
// Undefined if x <= 0
// ============================================================================

float test_log2_one() {
    // log2(1) should be 0
    return log2(rt(1.0));
}

// wasm.f32: builtin import has no f32 implementation — only Q32 builtin ids
// resolve, so the import cannot be lowered in f32 mode. Unblocks with M5.
// @unimplemented(wasm.f32)
// run: test_log2_one() ~= 0.0

float test_log2_two() {
    // log2(2) should be 1
    return log2(rt(2.0));
}

// @unimplemented(wasm.f32)
// run: test_log2_two() ~= 1.0

float test_log2_four() {
    // log2(4) should be 2
    return log2(rt(4.0));
}

// @unimplemented(wasm.f32)
// run: test_log2_four() ~= 2.0

float test_log2_eight() {
    // log2(8) should be 3
    return log2(rt(8.0));
}

// @unimplemented(wasm.f32)
// run: test_log2_eight() ~= 3.0

float test_log2_half() {
    // log2(0.5) should be -1
    return log2(rt(0.5));
}

// @unimplemented(wasm.f32)
// run: test_log2_half() ~= -1.0

float test_log2_sqrt_two() {
    // log2(√2) should be 0.5
    return log2(rt(1.4142135623730951));
}

// @unimplemented(wasm.f32)
// run: test_log2_sqrt_two() ~= 0.5

vec2 test_log2_vec2() {
    // Test with vec2
    return log2(vec2(rt(1.0), rt(2.0)));
}

// @unimplemented(wasm.f32)
// run: test_log2_vec2() ~= vec2(0.0, 1.0)

vec3 test_log2_vec3() {
    // Test with vec3
    return log2(vec3(rt(1.0), rt(2.0), rt(4.0)));
}

// @unimplemented(wasm.f32)
// run: test_log2_vec3() ~= vec3(0.0, 1.0, 2.0)

vec4 test_log2_vec4() {
    // Test with vec4
    return log2(vec4(rt(1.0), rt(2.0), rt(0.5), rt(0.25)));
}

// @unimplemented(wasm.f32)
// run: test_log2_vec4() ~= vec4(0.0, 1.0, -1.0, -2.0)




