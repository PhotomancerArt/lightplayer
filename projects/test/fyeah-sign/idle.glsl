layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec3 paletteRainbow(float t) {
    float r = 0.33333;
    vec3 v = abs(mod(fract(1.0 - t) + vec3(0.0, 1.0, 2.0) * r, 1.0) * 2.0 - 1.0);
    return v * v * (3.0 - 2.0 * v);
}

vec3 paletteCool(float t) {
    vec3 a = vec3(0.46, 0.50, 0.58);
    vec3 b = vec3(0.38, 0.36, 0.32);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.18, 0.30, 0.44);
    return clamp(a + b * cos(6.2831853 * (c * t + d)), 0.0, 1.0);
}

vec3 paletteWarm(float t) {
    vec3 a = vec3(0.50, 0.42, 0.34);
    vec3 b = vec3(0.42, 0.30, 0.24);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.00, 0.10, 0.24);
    return clamp(a + b * cos(6.2831853 * (c * t + d)), 0.0, 1.0);
}

vec3 applyPalette(float t, float palette) {
    float p = floor(palette + 0.001);
    if (p < 0.5) return paletteCool(t);
    if (p < 1.5) return paletteRainbow(t);
    return paletteWarm(t);
}

vec2 movingNoise(vec2 coord, float t) {
    vec2 gradient;
    float noise = lpfn_psrdnoise(
        coord + vec2(t * 0.030, -t * 0.020),
        vec2(0.0),
        t * 0.090,
        gradient,
        0u
    );
    float hue = mod(t * 0.055 + noise * 0.23 + dot(coord, vec2(0.018, -0.011)), 1.0);
    float edge = atan(gradient.y, gradient.x) * 0.15915494 + 0.5;
    float value = mix(0.38, 0.95, edge);
    return vec2(hue, value);
}

vec4 render(vec2 pos) {
    const vec2 REF_SIZE = vec2(32.0, 32.0);
    vec2 uv = pos / outputSize;
    vec2 virtCoord = pos * REF_SIZE / outputSize;
    vec2 center = REF_SIZE * 0.5;
    vec2 fromCenter = virtCoord - center;

    float zoom = mix(0.040, 0.070, 0.5 + 0.5 * sin(time * 0.32));
    float drift = sin(time * 0.18);
    vec2 coord = center + fromCenter * zoom + vec2(drift * 0.60, time * 0.075);

    vec2 tv = movingNoise(coord, time);
    float bands = 0.5 + 0.5 * sin((uv.x + uv.y) * 7.0 + time * 0.85 + tv.x * 6.2831853);
    float breath = 0.72 + 0.18 * sin(time * 0.75);

    float palettePhase = mod(time, 18.0) * 0.16666667;
    float palette = min(floor(palettePhase), 2.0);
    float nextPalette = palette + 1.0;
    if (nextPalette > 2.5) {
        nextPalette = 0.0;
    }
    float blend = smoothstep(0.78, 1.0, palettePhase - palette);

    vec3 color = mix(applyPalette(tv.x, palette), applyPalette(tv.x, nextPalette), blend);
    color *= mix(0.48, 1.0, bands) * tv.y * breath;
    color += paletteRainbow(fract(tv.x + 0.20)) * smoothstep(0.88, 1.0, bands) * 0.16;
    return vec4(clamp(color, 0.0, 1.0), 1.0);
}
