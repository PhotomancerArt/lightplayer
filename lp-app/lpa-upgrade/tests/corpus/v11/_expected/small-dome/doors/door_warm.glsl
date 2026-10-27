// Mini dome doors — the always-lit way-out glow.
//
// The radiance dome's doors were fixtures that never joined the show:
// steady warm panels people steer by. A slow breath keeps them alive
// without ever reading as content. Each door is a 9-lamp triangular
// polygon panel; rotating one by its stride (3 lamps — one side) in the
// PATCH is the "seated wrong" fix, and this shader stays oblivious.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;
layout(binding = 2) uniform float warmth;

const float TAU = 6.28318530718;

vec4 render_2d(vec2 pos) {
    // Two slow breaths per cycle, shallow on purpose.
    float breath = 0.9 + 0.1 * cos(phase * 2.0 * TAU);
    vec3 amber = vec3(1.0, 0.62, 0.24);
    vec3 white = vec3(1.0, 0.92, 0.80);
    vec3 color = mix(white, amber, warmth) * breath;
    return vec4(color, 1.0);
}
