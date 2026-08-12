// Peach leaves — a green plane with light walking across them.
//
// The leaves sit in a band across the top of the artwork, so the picture that
// matters to them varies mostly left to right. Both leaves are one fixture
// and one plane: the light reaches the far leaf when it gets there in SPACE,
// not when the wire does.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;

    // The walk crosses the pair of leaves once per cycle.
    float walk = 0.5 + 0.5 * sin((uv.x * 3.0 - phase * 2.0) * 6.2831853);
    // Veins run with it, finer and faster.
    float veins = 0.5 + 0.5 * sin((uv.x * 14.0 + uv.y * 6.0 + phase * 5.0) * 6.2831853);

    float level = 0.35 + 0.40 * walk + 0.25 * veins * veins;
    return vec4(vec3(0.08, 0.78, 0.26) * level, 1.0);
}
