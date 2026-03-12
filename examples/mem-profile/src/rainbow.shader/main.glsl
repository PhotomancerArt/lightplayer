// 0=heatmap, 1=rainbow, 2=fire, 3=cool, 4=warm (5s per palette, 1s lerp transition)

// Lygia heatmap: blue -> cyan -> green -> yellow -> red
vec3 paletteHeatmap(float t) {
    vec3 r = t * 2.1 - vec3(1.8, 1.14, 0.3);
    return clamp(1.0 - r * r, 0.0, 1.0);
}

// Lygia physical hue (neon rainbow)
vec3 paletteRainbow(float t) {
    float r = 0.33333;
    vec3 v = abs(mod(fract(1.0 - t) + vec3(0.0, 1.0, 2.0) * r, 1.0) * 2.0 - 1.0);
    return v * v * (3.0 - 2.0 * v);
}

// Lygia fire: black -> red -> orange -> yellow -> white
vec3 paletteFire(float t) {
    return clamp(vec3(1.0, 0.25, 0.0625) * exp(4.0 * t - 1.0), 0.0, 1.0);
}

// Iñigo Quílez parametric palette: cool blues/cyans
vec3 paletteCool(float t) {
    vec3 a = vec3(0.5, 0.5, 0.5);
    vec3 b = vec3(0.5, 0.5, 0.5);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.25, 0.25, 0.25);
    return clamp(a + b * cos(6.28318530718 * (c * t + d)), 0.0, 1.0);
}

// Iñigo Quílez parametric palette: warm oranges/reds
vec3 paletteWarm(float t) {
    vec3 a = vec3(0.5, 0.5, 0.5);
    vec3 b = vec3(0.5, 0.5, 0.5);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.0, 0.1, 0.2);
    return clamp(a + b * cos(6.28318530718 * (c * t + d)), 0.0, 1.0);
}

vec3 applyPalette(float t, float palette) {
    if (palette < 0.5) return paletteHeatmap(t);
    if (palette < 1.5) return paletteRainbow(t);
    if (palette < 2.5) return paletteFire(t);
    if (palette < 3.5) return paletteCool(t);
    return paletteWarm(t);
}

vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    // Palette cycle: 5s per palette, 1s smooth transition to next
    float cyclePhase = mod(time, 5.0);
    float palette = floor(mod(time * 0.2, 5.0));
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
    //vec2 tv = fbm_demo(scaledCoord, time);
    //vec2 tv = worley_demo(scaledCoord, time);

    vec3 rgb = mix(
        applyPalette(tv.x, palette),
        applyPalette(tv.x, nextPalette),
        blend
    ) * tv.y;
    return vec4(rgb, 1.0);
}

vec2 worley_demo(vec2 scaledCoord, float time) {
    float noiseValue = lpfx_worley(scaledCoord * 2, 0u) / 2 + 0.5;
    float t = (cos(noiseValue * 3.1415 + time) + 1.0) * 0.5;
    return vec2(t, 1.0);
}

vec2 fbm_demo(vec2 scaledCoord, float time) {
    float noiseValue = lpfx_fbm(scaledCoord, 3, 0u);
    float t = mod(time * 0.1 + (cos(noiseValue * 3.1415 + time) + 1.0) * 0.5 / 3.0, 1.0);
    return vec2(t, 1.0);
}

vec2 prsd_demo(vec2 scaledCoord, float time) {
    vec2 gradient;
    float noiseValue = lpfx_psrdnoise(
        scaledCoord,
        vec2(0.0),
        time,
        gradient
    );

    float hue = (cos(noiseValue * 3.1415 + time) + 1.0) * 0.5;
    float gradientAngle = atan(gradient.y, gradient.x) / (2.0 * 3.14159) + 0.5;
    float t = mod(time * 0.1 + hue / 3.0, 1.0);
    float v = mix(0.5, 1.0, gradientAngle);
    return vec2(t, v);
}
