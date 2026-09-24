// Colour Wipe — each palette colour in turn wipes across the piece.
//
// idea: WLED/NeoPixel "Color Wipe" (the Adafruit strandtest classic),
//       expanded to 2D with an angle and re-authored as a closed-form
//       front: no per-lamp state, identical at any frame rate.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   the front's soft edge and glint are LED-relative (about 1.5
//           lamps), so the edge is crisp on a fine piece and still smooth
//           on a coarse one.
//   motion: piece-relative — a wipe crosses the whole lamp box, edge to
//           edge in the chosen direction, in about 2.4 s, then the colour
//           holds for 3.6 s. Five colours per `wipePhase` period, exact
//           loop.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float angle;
layout(binding = 5) uniform float wipePhase;
layout(binding = 6) uniform sampler2D palette;

const float PI = 3.14159265;
const float WIPES = 5.0;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec3 wipeColour(float k) {
    // k / WIPES lands on the palette's stops when it has WIPES + 1 stops
    // (last = first); any other palette just gives evenly spaced samples.
    return pal(fract(k / WIPES));
}

vec4 render_2d(vec2 pos) {
    float th = angle * (PI / 180.0);
    vec2 dir = vec2(cos(th), sin(th));
    float pitch = max(patternPitch, 0.004);
    float span = max(abs(patternExtent.x * dir.x) + abs(patternExtent.y * dir.y), pitch);
    float s = dot(pos, dir) / span;

    float t = wipePhase * WIPES;
    float k = floor(t);
    float prog = smoothstep(0.0, 0.4, fract(t));

    // Edge half-width: 1.5 lamps (divided by `detail`: finer is crisper).
    float e = 1.5 * pitch / span / clamp(detail, 0.1, 8.0);
    float front = mix(-1.0 - 2.0 * e, 1.0 + 2.0 * e, prog);
    float m = 1.0 - smoothstep(front - e, front + e, s);

    vec3 fresh = wipeColour(k);
    vec3 old = wipeColour(k + WIPES - 1.0);
    vec3 c = mix(old, fresh, m);

    // A brighter glint riding the front while it moves.
    float d = (s - front) / e;
    float glint = exp(-min(d * d, 16.0)) * step(0.001, prog) * step(prog, 0.999);
    c = c * (1.0 + 0.6 * glint);
    return vec4(clamp(c, 0.0, 1.0), 1.0);
}
