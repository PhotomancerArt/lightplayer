// test run
//
// Integration-style checks mirroring examples/basic/src/rainbow.shader/main.glsl.
// Expectations are blessed from jit.q32; wasm.q32 must match within tolerance.

vec2 test() {
    vec2 gradient;
    lpfx_psrdnoise(
        vec2(1,2),
        vec2(0.0),
        1,
        gradient,
        0u
    );
    return gradient;
}

// run: test() ~= vec2(-4.4331207, 0.54551697) (tolerance: 0.002)
