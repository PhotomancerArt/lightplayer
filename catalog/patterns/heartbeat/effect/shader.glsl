// Heartbeat — the whole piece beats lub-dub, each beat spreading from the centre.
//
// idea: the heartbeat double pulse (WLED "Heartbeat"), re-authored from
//       scratch as a closed-form envelope in time, lagged by distance from
//       the centre so a beat visibly spreads instead of blinking.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   piece-relative — the lag is measured on distance normalised by
//           the lamp box's corner, so a beat takes the same fraction of a
//           beat to reach the farthest lamp on any piece.
//   motion: piece-relative in space, fixed tempo in time — about 64 beats
//           a minute (`beatPhase`), colour walking the palette once per
//           `huePhase` period.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float beatPhase;
layout(binding = 5) uniform float huePhase;
layout(binding = 6) uniform sampler2D palette;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

// Wrapped distance in beat-cycle units, so a pulse near 0 also lights
// just before 1.
float cyc(float t, float at) {
    float d = fract(t - at + 0.5) - 0.5;
    return d;
}

vec4 render_2d(vec2 pos) {
    float corner = max(length(patternExtent), max(patternPitch, 0.004));
    float r = length(pos) / corner;

    // The beat reaches the edge `lag` of a beat after the centre.
    float lag = 0.16 * clamp(detail, 0.1, 8.0);
    float t = beatPhase - r * lag;

    float d1 = cyc(t, 0.0) / 0.05;
    float d2 = cyc(t, 0.2) / 0.07;
    float lub = exp(-min(d1 * d1, 16.0));
    float dub = exp(-min(d2 * d2, 16.0)) * 0.65;
    float beat = max(lub, dub);

    // Falls off gently toward the edge so the centre leads.
    float reach = 1.0 - 0.35 * r;
    float lum = 0.07 + 0.93 * beat * reach;
    vec3 c = pal(fract(huePhase + 0.12 * r)) * lum;
    return vec4(clamp(c, 0.0, 1.0), 1.0);
}
