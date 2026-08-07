layout(binding = 0) uniform vec2 outputSize;
// Unbounded seconds: the fbm field is scrolled, not wrapped.
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float wavePhase;
layout(binding = 3) uniform float palettePhase;
// Authored palette (idle.json's `palette` slot) in place of the cosine
// function this shader used to carry. `palettePhase` still scrolls the
// lookup; the strip samples wrap=repeat, so it does not need folding.
layout(binding = 4) uniform sampler2D palette;

const float TAU = 6.2831853;

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec2 p = (uv - 0.5) * vec2(outputSize.x / outputSize.y, 1.0);
    float n = lpfn_fbm(p * 2.8 + vec2(time * 0.035, -time * 0.025), 3, 0u);
    float radius = dot(p, p);
    float wave = 0.5 + 0.5 * sin(TAU * wavePhase + n * 3.2 + radius * 5.5);
    vec3 color = texture(palette, vec2(palettePhase + wave * 0.35, 0.0)).rgb;
    color *= mix(0.20, 0.75, smoothstep(0.1, 0.95, wave));
    return vec4(color, 1.0);
}
