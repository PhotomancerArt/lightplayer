// Mini dome — rings rolling down from the apex, with a slow color sweep.
//
// The dome is mapped top-down with the apex at the canvas center, so
// radius in texture space IS height on the dome — exactly the zook-dome
// story at a tenth the scale. The patch document is where the five
// sectors land on real ports; this shader never knows and never cares,
// which is what makes re-plugging the dome a pointing job.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;
layout(binding = 2) uniform float bands;

const float TAU = 6.28318530718;

vec4 render_2d(vec2 pos) {
    vec2 p = pos / outputSize - 0.5;
    float r = length(p) * 2.0; // 0 at the apex, ~1 at the base ring
    float a = atan(p.y, p.x) / TAU + 0.5;

    // Rings rolling outward, `bands` visible at once, four passes per cycle.
    float ring = 0.5 + 0.5 * cos((r * bands - phase * 4.0) * TAU);
    ring = ring * ring;

    // A slow hue sweep around the dome, one turn per cycle.
    float sweep = 0.5 + 0.5 * cos((a - phase) * TAU);

    vec3 cool = vec3(0.10, 0.35, 0.65);
    vec3 warm = vec3(0.95, 0.55, 0.20);
    vec3 color = mix(cool, warm, sweep) * (0.25 + 0.75 * ring);
    return vec4(color, 1.0);
}
