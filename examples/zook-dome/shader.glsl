// Zook dome — rings from the apex plus a slow rotating beam.
//
// The dome is mapped top-down with the apex at the canvas center, so
// radius in texture space IS height on the dome: an expanding ring here
// is a wave rolling down the physical dome, crossing all five output
// channels without any per-channel configuration. The rotating beam
// sweeps around the dome the way a lighthouse would. Both effects ride
// integer harmonics of the one 30 s phasor, so every term is continuous
// when the phasor wraps.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

const float TAU = 6.28318530718;

vec4 render_2d(vec2 pos) {
    vec2 p = pos / outputSize - 0.5;
    float r = length(p) * 2.0; // 0 at the apex, ~1 at the base ring

    // Rings rolling down the dome: 3 visible bands, 6 passes per phasor.
    float rings = 0.5 + 0.5 * cos(TAU * (r * 3.0 - phase * 6.0));
    rings = rings * rings; // sharpen crests without a conditional

    // Lighthouse beam: unit direction rotating once per phasor. Using
    // dot(p, dir)/r instead of atan keeps the math cheap in fixed point.
    vec2 dir = vec2(cos(TAU * phase), sin(TAU * phase));
    float along = dot(p, dir) / max(r * 0.5, 0.02);
    float beam = smoothstep(0.55, 1.0, along);

    // Warm crests over a deep blue base; the beam lifts toward white.
    vec3 base = vec3(0.02, 0.05, 0.20);
    vec3 crest = vec3(1.00, 0.45, 0.10);
    vec3 color = base + crest * rings * (0.35 + 0.65 * (1.0 - r));
    color += vec3(0.9, 0.9, 1.0) * beam * 0.6;

    return vec4(color, 1.0);
}
