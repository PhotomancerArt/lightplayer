layout(binding = 0) uniform vec2 outputSize;
// Unbounded seconds: the psrdnoise field is scrolled, not wrapped.
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float zoomPhase;
layout(binding = 3) uniform float driftPhase;
layout(binding = 4) uniform float bandPhase;
layout(binding = 5) uniform float breathPhase;
// The three moods are authored palettes now (idle.json's `palette` slot
// cycles them), so the strip below replaces the cosine trio, the bare-float
// switcher, and the hand-rolled crossfade this shader used to carry. `u`
// may run past [0,1) — the strip samples wrap=repeat.
layout(binding = 6) uniform sampler2D palette;
layout(binding = 7) uniform float glow;

const float TAU = 6.2831853;

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
    // Front-panel knob: `glow` scales the highlight (default 0.5 reproduces
    // the original). The old `speed` multiplier is gone — the wrapped terms
    // carry their own periods now, and the scrolled noise rides `time`.
    vec2 uv = pos / outputSize;
    vec2 virtCoord = pos * REF_SIZE / outputSize;
    vec2 center = REF_SIZE * 0.5;
    vec2 fromCenter = virtCoord - center;

    float zoom = mix(0.040, 0.070, 0.5 + 0.5 * sin(TAU * zoomPhase));
    float drift = sin(TAU * driftPhase);
    vec2 coord = center + fromCenter * zoom + vec2(drift * 0.60, time * 0.075);

    vec2 tv = movingNoise(coord, time);
    float bands = 0.5 + 0.5 * sin((uv.x + uv.y) * 7.0 + TAU * bandPhase + tv.x * 6.2831853);
    float breath = 0.72 + 0.18 * sin(TAU * breathPhase);

    vec3 color = texture(palette, vec2(tv.x, 0.0)).rgb;
    color *= mix(0.48, 1.0, bands) * tv.y * breath;
    color += texture(palette, vec2(tv.x + 0.20, 0.0)).rgb
        * smoothstep(0.88, 1.0, bands) * (0.32 * glow);
    return vec4(clamp(color, 0.0, 1.0), 1.0);
}
