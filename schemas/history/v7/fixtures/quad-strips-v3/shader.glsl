// Quad-strip bring-up pattern: four horizontal bands, one per strip.
//
// Each band has its own base hue (band 1 red, 2 green, 3 blue, 4 amber) so a
// strip wired to the wrong channel — or a channel bleeding into its
// neighbour — is visible at a glance, and each band carries a bright chase
// dot moving at a band-specific speed and phase so per-channel animation
// independence is visible too. A dim base fill keeps every strip identifiably
// lit even where the dot is not.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

vec3 bandColor(float band) {
    if (band < 0.5) {
        return vec3(1.0, 0.0, 0.0);
    }
    if (band < 1.5) {
        return vec3(0.0, 1.0, 0.0);
    }
    if (band < 2.5) {
        return vec3(0.0, 0.0, 1.0);
    }
    return vec3(1.0, 0.6, 0.0);
}

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    float band = floor(uv.y * 4.0);

    // Band-specific chase: `phase` is one 20 s cycle and each band rides a
    // whole-number multiple of it (0.25, 0.35, 0.45, 0.55 Hz originally), so
    // the bands stay in the same relation without unbounded seconds.
    float head = fract(phase * (5.0 + band * 2.0) + band * 0.25);
    float d = abs(uv.x - head);
    d = min(d, 1.0 - d);
    float dot_i = smoothstep(0.12, 0.0, d);

    vec3 color = bandColor(band) * (0.12 + 0.88 * dot_i);
    return vec4(color, 1.0);
}
