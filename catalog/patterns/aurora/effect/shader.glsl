// Aurora — soft ribbons of light that sway and fold across the piece.
//
// family: Fields
// idea: aurora borealis curtains (WLED "Aurora" and the many shader-art auroras), re-authored from scratch as two noise-driven ribbons.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   piece-relative — heights are fractions of the piece's own
//           half-height (`patternExtent.y`), so a ribbon fills the band of
//           a choker as it fills a square; along x the sway has about one
//           fold per pattern unit at detail 1. On a straight strip
//           (extent.y = 0) the ribbons cross it as moving pools of light.
//   motion: piece-relative — the ribbons sway on a closed loop through the
//           noise (exact every `swayPhase` period) and their streaks drift
//           along x once per `streakPhase` period.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float swayPhase;
layout(binding = 5) uniform float streakPhase;
layout(binding = 6) uniform sampler2D palette;

const float TAU = 6.2831853;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

// One ribbon: brightness at height `yn` (−1…1 of the piece's half-height).
float ribbon(float x, float yn, float a, float seedOffset, float width) {
    vec3 q = vec3(x, cos(a) * 1.3 + seedOffset, sin(a) * 1.3);
    float centre = 0.62 * lpfn_snoise(q, 0u);
    float d = (yn - centre) / width;
    return exp(-min(d * d, 16.0));
}

vec4 render_2d(vec2 pos) {
    float det = clamp(detail, 0.1, 8.0);
    float half_h = max(patternExtent.y, 0.001);
    float yn = pos.y / half_h;
    float x = pos.x * 0.9 * det;
    float a = swayPhase * TAU;

    float r1 = ribbon(x, yn, a, 0.0, 0.42);
    float r2 = ribbon(x * 1.3 + 3.1, yn, a + 2.1, 7.7, 0.30);

    // Vertical curtain streaks, drifting slowly along the ribbon.
    float s = streakPhase * TAU;
    float streak = 0.55 + 0.45 * lpfn_snoise(vec3(pos.x * 5.5 * det, cos(s), sin(s)), 0u);

    float light = (r1 + 0.7 * r2) * streak;
    float hue = 0.35 + 0.35 * lpfn_snoise(vec2(x * 0.6 + 1.7, cos(a) * 0.8), 0u) + 0.15 * r2;

    vec3 c = pal(hue) * light + pal(0.0) * 0.08;
    return vec4(clamp(c, 0.0, 1.0), 1.0);
}
