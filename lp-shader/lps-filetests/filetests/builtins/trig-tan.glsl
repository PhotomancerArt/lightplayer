// test run

layout(binding = 0) uniform float u_runtime_zero;

float rt(float x) { return x + u_runtime_zero; }

// ============================================================================
// tan(): Tangent function
// ============================================================================

float test_tan_zero() {
    // tan(0) should be 0
    return tan(rt(0.0));
}

// run: test_tan_zero() ~= 0.0

float test_tan_pi_fourth() {
    // tan(π/4) should be 1
    return tan(rt(0.7853981633974483));
}

// run: test_tan_pi_fourth() ~= 1.0

float test_tan_pi_half() {
    // tan(π/2) should be undefined (very large positive)
    return tan(rt(1.5707963267948966));
}

// wgpu.f32: f32 GPU result diverges (undefined/edge-domain semantics)
// @unsupported(wgpu.f32)
// run: test_tan_pi_half() ~= 1.6331778728383844e16 (tolerance: 1e17)

float test_tan_pi() {
    // tan(π) should be 0
    return tan(rt(3.141592653589793));
}

// run: test_tan_pi() ~= 0.0 (tolerance: 0.01)

float test_tan_negative() {
    // tan(-π/4) should be -1
    return tan(rt(-0.7853981633974483));
}

// run: test_tan_negative() ~= -1.0

float test_tan_small() {
    // tan(0.1) should be approximately 0.10033467208545055
    return tan(rt(0.1));
}

// run: test_tan_small() ~= 0.10033467208545055

vec2 test_tan_vec2() {
    // Test with vec2
    return tan(vec2(rt(0.0), rt(0.7853981633974483)));
}

// run: test_tan_vec2() ~= vec2(0.0, 1.0)

vec3 test_tan_vec3() {
    // Test with vec3
    return tan(vec3(rt(0.0), rt(0.7853981633974483), rt(3.141592653589793)));
}

// run: test_tan_vec3() ~= vec3(0.0, 1.0, 0.0) (tolerance: 0.01)

vec4 test_tan_vec4() {
    // Test with vec4
    return tan(vec4(rt(0.0), rt(0.7853981633974483), rt(3.141592653589793), rt(-0.7853981633974483)));
}

// run: test_tan_vec4() ~= vec4(0.0, 1.0, 0.0, -1.0) (tolerance: 0.01)



