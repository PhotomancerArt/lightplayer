layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float speed;

vec3 paletteRainbow(float t) {
    float phase = fract(t);
    vec3 v = abs(mod(phase + vec3(0.00, 0.67, 0.33), 1.0) * 2.0 - 1.0);
    return clamp(v * v * (3.0 - 2.0 * v), 0.0, 1.0);
}

vec3 paletteParty(float t) {
    float phase = fract(t);
    vec3 red = vec3(1.00, 0.00, 0.12);
    vec3 yellow = vec3(1.00, 0.92, 0.00);
    vec3 cyan = vec3(0.00, 0.92, 1.00);
    vec3 violet = vec3(0.62, 0.00, 1.00);
    vec3 color = mix(red, yellow, smoothstep(0.00, 0.30, phase));
    color = mix(color, cyan, smoothstep(0.28, 0.68, phase));
    return mix(color, violet, smoothstep(0.64, 1.00, phase));
}

vec3 paletteFire(float t) {
    float phase = fract(t);
    vec3 red = vec3(1.00, 0.00, 0.04);
    vec3 orange = vec3(1.00, 0.32, 0.00);
    vec3 yellow = vec3(1.00, 0.86, 0.02);
    vec3 color = mix(red, orange, smoothstep(0.00, 0.56, phase));
    return mix(color, yellow, smoothstep(0.48, 1.00, phase));
}

vec3 applyPalette(float t, float palette) {
    float p = floor(palette + 0.001);
    if (p < 0.5) return paletteRainbow(t);
    if (p < 1.5) return paletteParty(t);
    return paletteFire(t);
}

float wheelDistance(float a, float b) {
    float d = abs(fract(a - b + 0.5) - 0.5);
    return d;
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec2 p = uv - 0.5;
    float aspect = outputSize.x / outputSize.y;
    p.x *= aspect;

    float angle = atan(p.y, p.x) * 0.15915494 + 0.5;
    float radius = dot(p, p);
    float rim = smoothstep(0.0784, 0.1600, radius) * (1.0 - smoothstep(0.3136, 0.4900, radius));

    float speedScale = max(speed, 0.0);
    float rotation = time * 0.115 * speedScale;
    float wheel = fract(angle + rotation);
    float palettePhase = mod(time * 0.055 * speedScale, 3.0);
    float palette = min(floor(palettePhase), 2.0);
    float nextPalette = palette + 1.0;
    if (nextPalette > 2.5) {
        nextPalette = 0.0;
    }
    float paletteBlend = smoothstep(0.92, 1.0, palettePhase - palette);

    float slice = fract(wheel * 1.18);
    vec3 color = mix(applyPalette(slice, palette), applyPalette(slice, nextPalette), paletteBlend);

    float darkA = 1.0 - smoothstep(0.090, 0.145, wheelDistance(wheel, 0.18));
    float darkB = 1.0 - smoothstep(0.075, 0.125, wheelDistance(wheel, 0.68));
    float level = 1.0 - max(darkA, darkB);
    color *= rim * level * 1.18;
    return vec4(clamp(color, 0.0, 1.0), 1.0);
}
