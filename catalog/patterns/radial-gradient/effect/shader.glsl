// Radial Gradient — rings of palette colour flowing outward from the centre.
//
// family: Gradients
// idea: the radial/"ripple from centre" palette gradient (WLED 2D "Colored Bursts" / radial fills in shader art), rebuilt from scratch as a mirrored ramp on the distance from the piece's centre.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   piece-relative — distance is measured from the centre of the
//           lamp box and normalised by the distance to its corner, so
//           `repeat` 1 is one ramp from the centre to the farthest lamp on
//           any piece. `detail` multiplies the repeat count.
//   motion: piece-relative — rings travel outward one ramp length per
//           `flowPhase` period.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float repeat;
layout(binding = 5) uniform float flowPhase;
layout(binding = 6) uniform sampler2D palette;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float corner = max(length(patternExtent), max(patternPitch, 0.004));
    float r = length(pos) / corner;

    float count = clamp(repeat, 0.25, 12.0) * clamp(detail, 0.1, 8.0);
    float u = r * count - flowPhase;

    float t = fract(u * 0.5) * 2.0;
    float tri = 1.0 - abs(t - 1.0);
    // Darken the troughs a little so the rings read as rings.
    float lum = 0.35 + 0.65 * smoothstep(0.0, 0.45, tri);
    return vec4(pal(0.03 + 0.9 * tri) * lum, 1.0);
}
