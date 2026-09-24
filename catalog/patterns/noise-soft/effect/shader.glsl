// Soft Noise — a slow, smooth noise field drifting through the palette.
//
// idea: the classic "noise through a palette" ambient field (FastLED
//       Noise/Pacifica lineage, WLED "Noise 2D"), re-authored from scratch.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   piece-relative — about one glow per pattern unit at detail 1,
//           so two to three glows across the long side of any piece.
//   motion: piece-relative — the field wanders on a closed loop through
//           the noise, about 0.13 units/s (a glow crosses the piece in
//           ~15 s), and its gradients turn in place. It loops exactly
//           every `flowPhase` period, so there is no unbounded clock.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float flowPhase;
layout(binding = 5) uniform sampler2D palette;

const float TAU = 6.2831853;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float f = 1.1 * clamp(detail, 0.1, 8.0);
    vec2 p = pos * f;

    // A closed loop through noise space: the drift never runs away.
    float a = flowPhase * TAU;
    vec2 wander = vec2(cos(a), sin(a)) * 1.8;

    vec2 g;
    float broad = lpfn_psrdnoise(p + wander, vec2(0.0), a * 2.0, g, 0u);
    float fine = lpfn_psrdnoise(p * 2.3 - wander.yx * 1.4, vec2(0.0), -a * 3.0, g, 0u);
    float v = broad * 0.78 + fine * 0.22;

    // Low values sit at the palette's dark end: the valleys go dark.
    float u = clamp(0.48 + 0.72 * v, 0.0, 1.0);
    float lum = smoothstep(0.02, 0.55, u);
    return vec4(pal(u * 0.92) * (0.15 + 0.85 * lum), 1.0);
}
