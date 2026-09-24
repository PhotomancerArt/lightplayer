// Linear Gradient — the palette laid across the piece at an angle, scrolling.
//
// idea: the plain scrolling palette gradient every LED controller has
//       (WLED "Palette", FastLED fill_palette); rebuilt as a 2D ramp with
//       an angle, a repeat count and a mirror so the scroll never jumps.
//
// Pattern space (`"coords": "pattern"`): `pos` is the lamps' box, centred,
// long side −1…1, y up.
//
// Rulers
//   size:   piece-relative — `repeat` 1 lays the ramp once across the
//           piece in the chosen direction, measured on the lamp box's own
//           span in that direction, whatever the piece's shape. `detail`
//           multiplies the repeat count.
//   motion: piece-relative — one ramp length per `scrollPhase` period.

layout(binding = 0) uniform vec2 patternExtent;
layout(binding = 1) uniform float patternPitch;
layout(binding = 2) uniform float lampCount;
layout(binding = 3) uniform float detail;
layout(binding = 4) uniform float angle;
layout(binding = 5) uniform float repeat;
layout(binding = 6) uniform float scrollPhase;
layout(binding = 7) uniform sampler2D palette;

const float PI = 3.14159265;

vec3 pal(float u) {
    return texture(palette, vec2(clamp(u, 0.004, 0.996), 0.0)).rgb;
}

vec4 render_2d(vec2 pos) {
    float th = angle * (PI / 180.0);
    vec2 dir = vec2(cos(th), sin(th));

    // Half the lamp box's span along `dir`: s runs −1…1 edge to edge.
    float span = abs(patternExtent.x * dir.x) + abs(patternExtent.y * dir.y);
    float s = dot(pos, dir) / max(span, max(patternPitch, 0.004));

    float count = clamp(repeat, 0.25, 12.0) * clamp(detail, 0.1, 8.0);
    float u = (0.5 + 0.5 * s) * count - scrollPhase;

    // Mirrored ramp (up then back down): seamless at every repeat.
    float t = fract(u * 0.5) * 2.0;
    float tri = 1.0 - abs(t - 1.0);
    return vec4(pal(0.04 + 0.88 * tri), 1.0);
}
