// test run

// Focused slice from examples/basic/shader.glsl. This keeps the native frontend honest about
// the palette vector math and LPFN signatures needed before the full basic shader is useful.

vec3 paletteWarm(float t) {
    vec3 a = vec3(0.5, 0.5, 0.5);
    vec3 b = vec3(0.5, 0.5, 0.5);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.0, 0.1, 0.2);
    return clamp(a + b * cos(6.28318530718 * (c * t + d)), 0.0, 1.0);
}

vec2 fbm_demo(vec2 scaledCoord, float time) {
    float noiseValue = lpfn_fbm(scaledCoord, 3, 0u);
    float t = mod(time * 0.1 + (cos(noiseValue * 3.1415 + time) + 1.0) * 0.5 / 3.0, 1.0);
    return vec2(t, 1.0);
}

vec2 prsd_demo(vec2 scaledCoord, float time) {
    vec2 gradient;
    float noiseValue = lpfn_psrdnoise(
        scaledCoord,
        vec2(0.0),
        time,
        gradient,
        0u
    );

    float hue = (cos(noiseValue * 3.1415 + time) + 1.0) * 0.5;
    float gradientAngle = atan(gradient.y, gradient.x) / (2.0 * 3.14159) + 0.5;
    float t = mod(time * 0.1 + hue / 3.0, 1.0);
    float v = mix(0.5, 1.0, gradientAngle);
    return vec2(t, v);
}

vec3 test_basic_palette_warm_quarter() {
    return paletteWarm(0.25);
}

// run: test_basic_palette_warm_quarter() ~= vec3(0.5, 0.20611572, 0.024475098) (tolerance: 0.002)
