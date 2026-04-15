// test run

// ============================================================================
// modf(): Modf function
// modf(x, out i) returns fractional part, sets i to integer part
// Both return value and i have same sign as x
// ============================================================================

vec2 test_modf_positive() {
    // modf(3.7) should return (0.7, 3.0)
    float i;
    float f = modf(3.7, i);
    return vec2(f, i);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_modf_positive() ~= vec2(0.7, 3.0)

vec2 test_modf_negative() {
    // modf(-2.3) should return (-0.3, -2.0)
    float i;
    float f = modf(-2.3, i);
    return vec2(f, i);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_modf_negative() ~= vec2(-0.3, -2.0)

vec2 test_modf_integer() {
    // modf(5.0) should return (0.0, 5.0)
    float i;
    float f = modf(5.0, i);
    return vec2(f, i);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_modf_integer() ~= vec2(0.0, 5.0)

vec2 test_modf_zero() {
    // modf(0.0) should return (0.0, 0.0)
    float i;
    float f = modf(0.0, i);
    return vec2(f, i);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_modf_zero() ~= vec2(0.0, 0.0)

vec2 test_modf_small() {
    // modf(0.1) should return (0.1, 0.0)
    float i;
    float f = modf(0.1, i);
    return vec2(f, i);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_modf_small() ~= vec2(0.1, 0.0)

vec4 test_modf_vec2() {
    // Test with vec2
    vec2 i;
    vec2 f = modf(vec2(3.7, -2.3), i);
    return vec4(f.x, f.y, i.x, i.y);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_modf_vec2() ~= vec4(0.7, -0.3, 3.0, -2.0)

vec4 test_modf_vec3() {
    // Test with vec3
    vec3 i;
    vec3 f = modf(vec3(1.5, -0.8, 4.0), i);
    return vec4(f.x, f.y, f.z, i.x);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32n.q32)
// run: test_modf_vec3() ~= vec4(0.5, -0.8, 0.0, 1.0)




