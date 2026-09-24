// Fireflies — a few soft glows drifting slowly over near-black.
//
// idea: fireflies on a summer night (WLED "Fireflies"-style sparse glows,
//       FastLED "Fire flies" sketches), re-authored from scratch: each fly
//       is a Lissajous path with whole-number frequencies, so it loops.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   LED-relative — a glow is about 2.2 lamps in radius on any piece.
//   motion: piece-relative — flies wander the whole lamp box on paths that
//           close once per `driftPhase` period (one or two laps per axis),
//           and each pulses 4–8 times per period on its own rhythm.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float count;
layout(binding = 5) uniform float driftPhase;
layout(binding = 6) uniform sampler2D palette;

const float TAU = 6.2831853;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float pitch = max(patternPitch, 0.004);
    float radius = 2.2 * pitch / clamp(detail, 0.1, 8.0);
    float n = clamp(floor(count + 0.5), 1.0, 12.0);
    vec2 roam = patternExtent * 0.92;

    vec3 c = pal(0.0) * 0.04;
    for (int i = 0; i < 12; i++) {
        float fi = float(i);
        if (fi >= n) {
            break;
        }
        vec2 k = vec2(fi * 0.53 + 0.17, fi * 0.29 + 0.61);
        float h1 = lpfn_random(k, 0u);
        float h2 = lpfn_random(k + vec2(2.7, 1.3), 0u);
        float h3 = lpfn_random(k + vec2(5.1, 3.9), 0u);
        float h4 = lpfn_random(k + vec2(7.3, 6.1), 0u);

        // Whole-number lap counts per axis keep every path closed.
        float fx = 1.0 + floor(h1 * 1.999);
        float fy = 1.0 + floor(h2 * 1.999);
        vec2 fly = vec2(
            sin(TAU * (driftPhase * fx + h3)),
            sin(TAU * (driftPhase * fy + h4) + 1.3 * sin(TAU * driftPhase))
        ) * roam;

        float d = length(pos - fly) / radius;
        float glow = exp(-min(d * d, 16.0));

        float beats = 4.0 + floor(h3 * 4.999);
        float pulse = 0.5 - 0.5 * cos(TAU * (driftPhase * beats + h4));
        pulse = pulse * pulse * pulse;

        c += pal(0.62 + 0.3 * h1) * glow * pulse;
    }
    return vec4(clamp(c, 0.0, 1.0), 1.0);
}
