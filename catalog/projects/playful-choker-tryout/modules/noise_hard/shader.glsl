// Hard Noise — noise cut into flat bands of palette colour with crisp edges.
//
// family: Fields
// idea: posterized noise / "contour map" looks (toon-shaded noise, the stepped-noise palette effects common to LED firmware), re-authored from scratch.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   piece-relative — about 1.7 noise features per pattern unit at
//           detail 1, finer than Soft Noise, so each band is a clear shape.
//           The band edge is LED-relative: it softens over half a pitch so
//           an edge lamp reads as one colour or the other, not a smear.
//   motion: piece-relative — the field slides diagonally ~0.09 units/s on a
//           closed loop (exact every `slidePhase` period).

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float steps;
layout(binding = 5) uniform float slidePhase;
layout(binding = 6) uniform sampler2D palette;

const float TAU = 6.2831853;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float f = 1.7 * clamp(detail, 0.1, 8.0);
    vec2 p = pos * f;

    float a = slidePhase * TAU;
    // An elongated loop: mostly a diagonal slide, back along a parallel lane.
    vec2 slide = vec2(cos(a) * 2.2, sin(a) * 0.8);

    vec2 g;
    float n = lpfn_psrdnoise(p + slide, vec2(0.0), a, g, 0u);
    float u = clamp(0.5 + 0.62 * n, 0.0, 0.999);

    float s = clamp(floor(steps + 0.5), 2.0, 12.0);
    float x = u * s;
    float band = floor(x);

    // Soften each edge over about half a lamp: the noise changes by
    // |g| * f per pattern unit, so half a pitch is this much of a band.
    float pitch = max(patternPitch, 0.004);
    float soft = clamp(0.62 * s * length(g) * f * pitch * 0.5, 0.02, 0.45);
    float k = smoothstep(1.0 - soft, 1.0, fract(x));
    float q = (band + k) / (s - 1.0);

    // Lowest band is dark ground; the rest step up through the palette,
    // stopping short of its brightest end.
    vec3 c = pal(0.06 + 0.84 * q);
    float lum = mix(0.12, 1.0, clamp(q * 1.6, 0.0, 1.0));
    return vec4(c * lum, 1.0);
}
