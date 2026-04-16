// noise.fx — demo effect: lpfn noise + palettes + cycling (M0 compile check).
// Entry point for tooling / previews.

layout(binding = 0) uniform float speed;
layout(binding = 0) uniform float zoom;
layout(binding = 0) uniform int noise_fn;
layout(binding = 0) uniform int palette;
layout(binding = 0) uniform bool cycle_palettes;
layout(binding = 0) uniform float cycle_time_s;

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

vec3 applyPalette(float t, int pal) {
    float p = float(pal);
    if (p < 0.5) return paletteHeatmap(t);
    if (p < 1.5) return paletteRainbow(t);
    if (p < 2.5) return paletteFire(t);
    if (p < 3.5) return paletteCool(t);
    return paletteWarm(t);
}

vec2 prsd_demo(vec2 scaledCoord, float t) {
    vec2 gradient;
    float noiseValue = lpfn_psrdnoise(
        scaledCoord,
        vec2(0.0),
        t,
        gradient,
        0u
    );

    float hue = (cos(noiseValue * 3.1415 + t) + 1.0) * 0.5;
    float gradientAngle = atan(gradient.y, gradient.x) / (2.0 * 3.14159) + 0.5;
    float tt = mod(t * 0.1 + hue / 3.0, 1.0);
    float v = mix(0.5, 1.0, gradientAngle);
    return vec2(tt, v);
}

vec2 worley_demo(vec2 scaledCoord, float t) {
    float w = lpfn_worley(scaledCoord, 0u);
    float hue = (cos(w * 6.28318 + t * 0.5) + 1.0) * 0.5;
    float tt = mod(t * 0.1 + hue * 0.3, 1.0);
    float v = mix(0.5, 1.0, abs(w));
    return vec2(tt, v);
}

vec2 fbm_demo(vec2 scaledCoord, float t) {
    float n = lpfn_fbm(scaledCoord, 4, 0u);
    float hn = n * 0.5 + 0.5;
    float tt = mod(t * 0.08 + hn, 1.0);
    float v = mix(0.55, 1.0, hn);
    return vec2(tt, v);
}

vec2 pick_noise(vec2 scaledCoord, float t) {
    if (noise_fn == 0) return prsd_demo(scaledCoord, t);
    if (noise_fn == 1) return worley_demo(scaledCoord, t);
    return fbm_demo(scaledCoord, t);
}

vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
    float t = time * speed;
    float s = 0.05 * zoom;
    vec2 center = outputSize * 0.5;
    vec2 dir = fragCoord - center;
    vec2 scaledCoord = center + dir * s;

    vec2 tv = pick_noise(scaledCoord, t);

    vec3 col;
    if (cycle_palettes) {
        float period = max(cycle_time_s, 0.001);
        float u = mod(t / period, 1.0);
        float a = floor(u * 5.0);
        float b = mod(a + 1.0, 5.0);
        float w = fract(u * 5.0);
        w = smoothstep(0.0, 1.0, w);
        vec3 c0 = applyPalette(tv.x, int(a));
        vec3 c1 = applyPalette(tv.x, int(b));
        col = mix(c0, c1, w);
    } else {
        col = applyPalette(tv.x, palette);
    }

    return vec4(col * tv.y, 1.0);
}
