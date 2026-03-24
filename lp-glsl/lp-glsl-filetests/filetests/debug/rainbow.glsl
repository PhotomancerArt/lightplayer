// test run
//
// Integration-style checks mirroring examples/basic/src/rainbow.shader/main.glsl.
// Expectations are blessed from cranelift.q32; wasm.q32 must match within tolerance.

const bool CYCLE_PALETTE = true;

vec3 paletteHeatmap(float t) {
    vec3 r = t * 2.1 - vec3(1.8, 1.14, 0.3);
    return clamp(1.0 - r * r, 0.0, 1.0);
}

vec3 paletteRainbow(float t) {
    float r = 0.33333;
    vec3 v = abs(mod(fract(1.0 - t) + vec3(0.0, 1.0, 2.0) * r, 1.0) * 2.0 - 1.0);
    return v * v * (3.0 - 2.0 * v);
}

vec3 paletteFire(float t) {
    return clamp(vec3(1.0, 0.25, 0.0625) * exp(4.0 * t - 1.0), 0.0, 1.0);
}

vec3 paletteCool(float t) {
    vec3 a = vec3(0.5, 0.5, 0.5);
    vec3 b = vec3(0.5, 0.5, 0.5);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.25, 0.25, 0.25);
    return clamp(a + b * cos(6.28318530718 * (c * t + d)), 0.0, 1.0);
}

vec3 paletteWarm(float t) {
    vec3 a = vec3(0.5, 0.5, 0.5);
    vec3 b = vec3(0.5, 0.5, 0.5);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.0, 0.1, 0.2);
    return clamp(a + b * cos(6.28318530718 * (c * t + d)), 0.0, 1.0);
}

vec3 applyPalette(float t, float palette) {
    float p = floor(palette + 0.001);
    if (p < 0.5) return paletteHeatmap(t);
    if (p < 1.5) return paletteRainbow(t);
    if (p < 2.5) return paletteFire(t);
    if (p < 3.5) return paletteCool(t);
    return paletteWarm(t);
}

vec2 prsd_demo(vec2 scaledCoord, float time) {
    vec2 gradient;
    float noiseValue = lpfx_psrdnoise(
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

vec4 rainbow_main(vec2 fragCoord, vec2 outputSize, float time) {
    float cyclePhase = mod(time, 5.0);
    float palette = min(floor(mod(time * 0.2, 5.0)), 4.0);
    float nextPalette = mod(palette + 1.0, 5.0);
    float blend = smoothstep(4.0, 5.0, cyclePhase);

    float panSpeed = .3;
    float pan = mix(1.0, 8.0, 0.5 * (sin(time * panSpeed) + 1.0));

    float scaleSpeed = .7;
    float scale = mix(.04, .06, 0.5 * (sin(time * scaleSpeed) + 1.0));

    vec2 center = outputSize * 0.5;
    vec2 dir = fragCoord - center;
    vec2 scaledCoord = center + dir * scale;

    vec2 tv = prsd_demo(scaledCoord, time);

    if (CYCLE_PALETTE) {
        return vec4(mix(
            applyPalette(tv.x, palette),
            applyPalette(tv.x, nextPalette),
            blend
        ) * tv.y, 1.0);
    } else {
        return vec4(applyPalette(tv.x, 0) * tv.y, 1.0);
    }
}

vec3 test_rainbow_palette_heatmap_0() {
    return paletteHeatmap(0.0);
}

// run: test_rainbow_palette_heatmap_0() ~= vec3(0.0, 0.0, 0.91) (tolerance: 0.002)

vec3 test_rainbow_palette_heatmap_half() {
    return paletteHeatmap(0.5);
}

// run: test_rainbow_palette_heatmap_half() ~= vec3(0.4375, 0.99191284, 0.4375) (tolerance: 0.002)

vec3 test_rainbow_palette_rainbow_quarter() {
    return paletteRainbow(0.25);
}

// run: test_rainbow_palette_rainbow_quarter() ~= vec3(0.5, 0.9259186, 0.0740509) (tolerance: 0.002)

vec2 test_rainbow_prsd_center_t1() {
    return prsd_demo(vec2(32.0, 32.0), 1.0);
}

// run: test_rainbow_prsd_center_t1() ~= vec2(0.35671997, 0.78215027) (tolerance: 0.002)

vec4 test_rainbow_main_center_t0() {
    return rainbow_main(vec2(32.0, 32.0), vec2(64.0, 64.0), 0.0);
}

// run: test_rainbow_main_center_t0() ~= vec4(0.0, 0.5663605, 0.5899811, 1.0) (tolerance: 0.002)

vec4 test_rainbow_main_center_t25() {
    return rainbow_main(vec2(32.0, 32.0), vec2(64.0, 64.0), 2.5);
}

// run: test_rainbow_main_center_t25() ~= vec4(0.0, 0.6333008, 0.8231659, 1.0) (tolerance: 0.002)

vec4 test_rainbow_main_corner_t5() {
    return rainbow_main(vec2(0.0, 0.0), vec2(64.0, 64.0), 5.0);
}

// run: test_rainbow_main_corner_t5() ~= vec4(0.26849365, 0.6054535, 0.2654419, 1.0) (tolerance: 0.002)
