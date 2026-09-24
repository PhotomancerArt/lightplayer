// Spiral — pinwheel arms of palette colour turning around the centre.
//
// idea: conic gradients and pinwheel/spiral shader art (WLED 2D "Spiral"
//       lineage), re-authored from scratch: angle times arm count plus a
//       twist proportional to the distance from the centre.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   piece-relative — the twist is measured on distance normalised
//           by the lamp box's corner distance, so an arm makes the same
//           fraction of a turn between the centre and the farthest lamp on
//           any piece. `detail` multiplies the twist.
//   motion: piece-relative — the pinwheel turns one arm spacing per
//           `turnPhase` period, and the colours rotate through the palette
//           once per `huePhase` period.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float arms;
layout(binding = 5) uniform float turnPhase;
layout(binding = 6) uniform float huePhase;
layout(binding = 7) uniform sampler2D palette;

const float TAU = 6.2831853;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float corner = max(length(patternExtent), max(patternPitch, 0.004));
    float r = length(pos) / corner;

    // atan(0, 0) is undefined; nudge x so the centre lamp has an angle.
    float ang = atan(pos.y, pos.x + 0.0001) / TAU;
    float n = clamp(floor(arms + 0.5), 1.0, 8.0);
    float twist = 1.2 * clamp(detail, 0.1, 8.0);

    float u = ang * n + r * twist - turnPhase;
    // Arms peak half-way between whole values of u; every term of u
    // wraps by a whole number (the atan seam by n, the phasor by 1), so
    // neither the seam nor the phasor's wrap shows.
    float arm = 0.5 - 0.5 * cos(TAU * u);
    arm = arm * arm;

    // Colour walks the palette once around the circle and slowly with
    // time; this palette is a closed loop, so its wrap is seamless too.
    float hue = fract(ang + huePhase + r * 0.15);
    // Lamps right at the centre are a soft mix rather than a hard seam.
    float centre = smoothstep(0.0, 0.12, r);
    float lum = mix(0.45, arm, centre);
    return vec4(pal(hue) * (0.06 + 0.94 * lum), 1.0);
}
